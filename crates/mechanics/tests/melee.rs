//! Auto-attacks and what triggers off them, on a made-up melee kit grafted
//! onto the checked-in Elemental data: rage from swings, poisons on
//! weapon hits, dual-wield misses, and Skyfury's extra swings.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, Readiness, RunSetup, SeatSetup,
    StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::aura::{AuraDef, RefreshRule};
use portunus_gamedata::effect::{
    Coefficient, Effect, EffectTarget, ListenFor, Listener, ModKind, ModScope, Modifier, ProcChance,
};
use portunus_gamedata::item::{WeaponDef, WeaponHand};
use portunus_gamedata::spell::{CastKind, CooldownDef, GcdDef, SpellDef, Targeting};
use portunus_gamedata::stats::{ResourceDef, ResourceKind, SchoolMask, Stat};
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{Kits, PartyMechanics};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

const KIT: AuraId = AuraId(900_101);
const SKYFURY: AuraId = AuraId(462_854);
const STRIKE: SpellId = SpellId(900_102);
const COMBAT_START: SimTime = SimTime(10_000);

struct Fixture {
    data: GameData,
    enemies: Arc<EnemyData>,
    template: ActorTemplate,
    sampler: Sampler,
}

fn weapon(speed: u32, damage: f64) -> WeaponDef {
    WeaponDef {
        speed: SimDuration(speed),
        min_damage: damage,
        max_damage: damage,
        ranged: false,
    }
}

fn modifier(kind: ModKind, value: f64) -> Modifier {
    Modifier {
        scope: ModScope::All,
        kind,
        value,
        per_stack: false,
        condition: None,
    }
}

fn listener(on: ListenFor, chance: ProcChance, effects: Vec<Effect>) -> Listener {
    Listener {
        on,
        chance,
        internal_cooldown: None,
        per_unit: false,
        shared_with: None,
        condition: None,
        effects,
    }
}

fn damage(amount: f64, school: SchoolMask) -> Effect {
    Effect::Damage {
        amount: Coefficient::Flat(amount),
        school,
        target: EffectTarget::Target,
        aoe: None,
    }
}

/// A melee seat with no crits (so every hit of a hand lands for the same
/// amount) and no group buffs, holding `kit` as a passive aura, at a dummy
/// that won't die.
fn fixture(main_hand: WeaponDef, off_hand: Option<WeaponDef>, kit: Vec<Listener>) -> Fixture {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let mut data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    data.classes.clear();
    let mut enemies: EnemyData =
        EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    for e in enemies.enemies.values_mut() {
        e.health = 1_000_000_000_000;
    }
    let loadout: Loadout = read_ron(&dir.join("loadouts/elemental.ron")).unwrap();
    let mut template = Compiler.compile(&data, &loadout).unwrap();
    template.abilities.retain(|s| data.spells[s].hostile);
    template.melee = true;
    template.main_hand = Some(main_hand);
    template.off_hand = off_hand;
    template.resources.push(ResourceDef {
        kind: ResourceKind::Rage,
        max: 100.0,
        initial: 0.0,
        regen_per_sec: 0.0,
        regen_hasted: false,
    });
    data.auras.insert(
        KIT,
        AuraDef {
            id: KIT,
            name: "melee kit".into(),
            duration: None,
            max_stacks: 1,
            refresh: RefreshRule::Replace,
            periodic: None,
            value: None,
            modifiers: vec![modifier(ModKind::CritChanceAdd, -1000.0)],
            listeners: kit,
            overrides: Vec::new(),
            on_expire: Vec::new(),
            cancelable: false,
            blocked_by: None,
        },
    );
    template.passive_auras.push(KIT);
    let mut spec: ScenarioSpec = read_ron(&dir.join("scenarios/target_dummy.ron")).unwrap();
    spec.pulls[0].health = Dist::Fixed(1.0);
    let enemies = Arc::new(enemies);
    let sampler = Sampler::new(spec, Arc::clone(&enemies)).unwrap();
    Fixture {
        data,
        enemies,
        template,
        sampler,
    }
}

impl Fixture {
    fn setup(&self, seed: u64) -> RunSetup {
        RunSetup {
            run: Arc::new(self.sampler.sample(Seed(seed))),
            data: Arc::new(self.data.clone()),
            enemies: Arc::clone(&self.enemies),
            seats: vec![SeatSetup {
                template: self.template.clone(),
                latency: Latency {
                    anticipated: Dist::Fixed(SimDuration::ZERO),
                    reaction: Dist::Fixed(SimDuration::ZERO),
                    cast_lag: Dist::Fixed(SimDuration::ZERO),
                },
            }],
            externals: Externals::default(),
            record_trace: true,
        }
    }

    fn kernel(&self, seed: u64) -> Kernel<PartyMechanics> {
        let setup = self.setup(seed);
        let mechanics = PartyMechanics::new(&setup, Arc::new(Kits::standard())).unwrap();
        Kernel::new(setup, mechanics).unwrap()
    }
}

/// Answer with `policy` until `until`, waiting out the rest of the time.
fn run_until(
    k: &mut Kernel<PartyMechanics>,
    until: SimTime,
    mut policy: impl FnMut(&Kernel<PartyMechanics>) -> Option<Choice>,
) {
    loop {
        match k.advance().unwrap() {
            Step::Done(_) => return,
            Step::Decide(req) => {
                if req.now >= until {
                    return;
                }
                let choice = policy(k).unwrap_or(Choice::Wait(Wait::Until(until)));
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
}

fn idle(_: &Kernel<PartyMechanics>) -> Option<Choice> {
    None
}

/// Damage the seat dealt, by school, as `(time, amount, spell)`.
fn dealt(trace: &[TraceRecord], school: SchoolMask) -> Vec<(SimTime, u64, Option<SpellId>)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.source == ActorId(0) && d.school == school => {
                Some((r.time, d.amount, d.spell))
            }
            _ => None,
        })
        .collect()
}

/// White hits, split into main and off hand by amount (the main hand hits
/// harder).
fn white_hits(trace: &[TraceRecord]) -> (Vec<SimTime>, Vec<SimTime>) {
    let hits: Vec<_> = dealt(trace, SchoolMask::PHYSICAL)
        .into_iter()
        .filter(|h| h.2.is_none())
        .collect();
    let amounts: BTreeMap<u64, usize> = hits.iter().fold(BTreeMap::new(), |mut m, h| {
        *m.entry(h.1).or_default() += 1;
        m
    });
    let main = *amounts.keys().last().unwrap();
    let (mh, oh): (Vec<_>, Vec<_>) = hits.iter().partition(|h| h.1 == main);
    let times = |v: Vec<&(SimTime, u64, Option<SpellId>)>| v.into_iter().map(|h| h.0).collect();
    (times(mh), times(oh))
}

/// The swing interval of a hand that rarely misses twice in a row: its
/// shortest gap between distinct times.
fn interval(times: &[SimTime]) -> u32 {
    times
        .windows(2)
        .map(|w| (w[1] - w[0]).millis())
        .filter(|&gap| gap > 0)
        .min()
        .unwrap()
}

#[test]
fn swings_build_rage_by_weapon_speed_and_dual_wielders_miss() {
    let f = fixture(
        weapon(2600, 1000.0),
        Some(weapon(1300, 100.0)),
        vec![
            listener(
                ListenFor::Swing { hand: None },
                ProcChance::Always,
                vec![Effect::GainResource {
                    kind: ResourceKind::Rage,
                    amount: Coefficient::WeaponSpeed(1.0),
                }],
            ),
            // A poison on main-hand hits.
            listener(
                ListenFor::WeaponHit {
                    hand: Some(WeaponHand::MainHand),
                },
                ProcChance::Flat(0.3),
                vec![damage(7.0, SchoolMask::NATURE)],
            ),
        ],
    );
    let mut k = f.kernel(1);
    let early = COMBAT_START + SimDuration(20_001);
    run_until(&mut k, early, idle);
    let trace = k.drain_trace();
    let (mh, oh) = white_hits(&trace);
    let rage = k
        .state()
        .resource(ActorId(0), ResourceKind::Rage)
        .unwrap()
        .value;
    let want = 2.6 * mh.len() as f64 + 1.3 * oh.len() as f64;
    assert!((rage - want).abs() < 1e-6, "{rage} rage, want {want}");
    assert!(rage < 100.0);

    // Pooled over a few runs: about a thousand swings.
    let late = COMBAT_START + SimDuration(600_000);
    let (mut tried, mut landed, mut main, mut poisons) = (0.0, 0.0, 0.0, 0.0);
    for seed in 1..=5 {
        let mut k = f.kernel(seed);
        run_until(&mut k, late, idle);
        let trace = k.drain_trace();
        let (mh, oh) = white_hits(&trace);
        let tries = |every: u32| f64::from((late - COMBAT_START).millis() / every);
        tried += tries(interval(&mh)) + tries(interval(&oh));
        landed += (mh.len() + oh.len()) as f64;
        main += mh.len() as f64;
        poisons += dealt(&trace, SchoolMask::NATURE).len() as f64;
    }
    let missed = 1.0 - landed / tried;
    assert!((0.16..0.22).contains(&missed), "{missed} missed");
    let rate = poisons / main;
    assert!((0.25..0.35).contains(&rate), "{rate} poisons per hit");
}

#[test]
fn weapon_strikes_trigger_weapon_hits_but_not_swings() {
    let mut f = fixture(
        weapon(2600, 1000.0),
        None,
        vec![
            listener(
                ListenFor::Swing { hand: None },
                ProcChance::Always,
                vec![damage(1.0, SchoolMask::SHADOW)],
            ),
            listener(
                ListenFor::WeaponHit {
                    hand: Some(WeaponHand::MainHand),
                },
                ProcChance::Always,
                vec![damage(1.0, SchoolMask::ARCANE)],
            ),
        ],
    );
    f.data.spells.insert(
        STRIKE,
        SpellDef {
            id: STRIKE,
            name: "strike".into(),
            school: SchoolMask::PHYSICAL,
            cast: CastKind::Instant,
            gcd: Some(GcdDef {
                base: SimDuration(1500),
                hasted: true,
                floor: SimDuration(750),
            }),
            cooldown: Some(CooldownDef {
                duration: SimDuration(6000),
                charges: 1,
                hasted: false,
                category: None,
            }),
            costs: Vec::new(),
            targeting: Targeting::Enemy,
            hostile: true,
            range: None,
            speed: None,
            min_travel: SimDuration::ZERO,
            rolls_on_impact: false,
            castable_while_moving: false,
            usable_while_casting: false,
            weapon: Some(WeaponHand::MainHand),
            effects: vec![damage(5000.0, SchoolMask::PHYSICAL)],
        },
    );
    f.template.abilities.insert(STRIKE);
    let mut k = f.kernel(2);
    run_until(&mut k, COMBAT_START + SimDuration(60_000), |k| {
        let ready = k.legal(Seat(0)).abilities.get(&STRIKE) == Some(&Readiness::Now);
        Some(if ready {
            Choice::Cast {
                ability: STRIKE,
                target: TargetSel::Primary,
                opts: CastOpts::default(),
            }
        } else {
            Choice::Wait(Wait::NextEvent)
        })
    });
    let trace = k.drain_trace();
    let physical = dealt(&trace, SchoolMask::PHYSICAL);
    let strikes = physical.iter().filter(|h| h.2 == Some(STRIKE)).count();
    let swings = physical.iter().filter(|h| h.2.is_none()).count();
    assert!(strikes >= 9, "{strikes} strikes");
    // A single weapon never misses.
    assert_eq!(swings, 60_000 / 2600 + 1);
    assert_eq!(dealt(&trace, SchoolMask::SHADOW).len(), swings);
    assert_eq!(dealt(&trace, SchoolMask::ARCANE).len(), swings + strikes);
}

#[test]
fn skyfury_swings_again_without_resetting_the_timer() {
    let mut f = fixture(weapon(2600, 1000.0), None, Vec::new());
    f.template.passive_auras.push(SKYFURY);
    let mut k = f.kernel(3);
    let end = COMBAT_START + SimDuration(600_000);
    run_until(&mut k, end, idle);
    let trace = k.drain_trace();
    let (hits, _) = white_hits(&trace);
    let mut at: BTreeMap<SimTime, usize> = BTreeMap::new();
    for t in &hits {
        *at.entry(*t).or_default() += 1;
    }
    // The timer keeps its rhythm; extra swings land alongside a swing and
    // never chain.
    let swings = at.len();
    assert_eq!(
        swings,
        ((end - COMBAT_START).millis() / interval(&hits) + 1) as usize
    );
    assert!(at.values().all(|&n| n <= 2));
    let extra = (hits.len() - swings) as f64 / swings as f64;
    assert!((0.14..0.26).contains(&extra), "{extra} extra per swing");
}

#[test]
fn battle_shout_multiplies_attack_power() {
    let mut f = fixture(weapon(2600, 1000.0), None, Vec::new());
    let spec = f.template.spec;
    f.data.specs.get_mut(&spec).unwrap().primary_stat = Stat::Agility;
    f.template.stats.0.insert(Stat::Agility, 2000.0);
    let ap = |f: &Fixture| {
        let k = f.kernel(4);
        let m = PartyMechanics::new(&f.setup(4), Arc::new(Kits::standard())).unwrap();
        m.math().derived(k.state(), ActorId(0)).attack_power
    };
    let before = ap(&f);
    f.template.passive_auras.push(AuraId(6673));
    let after = ap(&f);
    assert!(before > 0.0);
    assert!((after / before - 1.05).abs() < 1e-9, "{before} -> {after}");
}
