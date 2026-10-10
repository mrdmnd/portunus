//! Auto-attacks and what triggers off them, on a made-up melee kit grafted
//! onto the checked-in Elemental data: rage from swings, poisons on
//! weapon hits, dual-wield misses, Skyfury's extra swings, and a made-up
//! druid's forms and stealth. Also casting (channels, empowers, shared
//! cooldowns, strikes with both hands) and aura values (thresholds, caps,
//! absorbs).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{
    ActorId, AuraId, Dist, EnemyKey, EventName, Seat, Seed, SimDuration, SimTime, SpellId, Trigger,
};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, Readiness, RunSetup, SeatSetup,
    StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::aura::{
    AuraDef, AuraValue, AuraValueKind, BankDraw, FormDef, Periodic, RefreshRule, StealthDef,
};
use portunus_gamedata::effect::{
    Coefficient, CooldownChange, Effect, EffectTarget, ListenFor, Listener, ModKind, ModScope,
    Modifier, Predicate, ProcChance,
};
use portunus_gamedata::enemy::{EnemyAction, EnemyKind, EnemyRule, EnemySubject};
use portunus_gamedata::item::{WeaponDef, WeaponHand};
use portunus_gamedata::spell::{CastKind, CooldownDef, GcdDef, Requirement, SpellDef, Targeting};
use portunus_gamedata::stats::{Cost, ResourceDef, ResourceKind, SchoolMask, SpendScaling, Stat};
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
        ignores_armor: false,
        hand: None,
        per_count: None,
        unmodified: false,
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
        recharge: None,
        out_of_combat: None,
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
            form: None,
            stealth: None,
            ends_with: None,
            persists_through_death: false,
            unique_per_source: false,
            prevents_death: None,
            ground: None,
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
            requires: Vec::new(),
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

const TWIN: SpellId = SpellId(900_103);

/// A strike with each hand: weapon damage, then `WeaponHit` listeners for
/// its own hand only.
#[test]
fn strikes_with_both_hands_hit_with_each_weapon() {
    let on_hit = |hand, school| {
        listener(
            ListenFor::WeaponHit { hand: Some(hand) },
            ProcChance::Always,
            vec![damage(1.0, school)],
        )
    };
    let kit = vec![
        on_hit(WeaponHand::MainHand, SchoolMask::NATURE),
        on_hit(WeaponHand::OffHand, SchoolMask::FROST),
    ];
    let off_hand = Some(weapon(1300, 100.0));
    // What a white hit of each hand deals.
    let f = fixture(weapon(2600, 1000.0), off_hand, kit.clone());
    let mut k = f.kernel(1);
    run_until(&mut k, COMBAT_START + SimDuration(20_000), idle);
    let white: Vec<u64> = dealt(&k.drain_trace(), SchoolMask::PHYSICAL)
        .into_iter()
        .filter(|h| h.2.is_none())
        .map(|h| h.1)
        .collect();
    let (mh, oh) = (*white.iter().max().unwrap(), *white.iter().min().unwrap());
    assert!(mh > 2 * oh, "{mh} vs {oh}");

    let strike = |hand| Effect::Damage {
        amount: Coefficient::WeaponDamage(1.0),
        school: SchoolMask::PHYSICAL,
        target: EffectTarget::Target,
        aoe: None,
        ignores_armor: false,
        hand: Some(hand),
        per_count: None,
        unmodified: false,
    };
    let run = |off_hand: Option<WeaponDef>| {
        let mut f = fixture(weapon(2600, 1000.0), off_hand, kit.clone());
        f.template.melee = false;
        let effects = vec![strike(WeaponHand::MainHand), strike(WeaponHand::OffHand)];
        let s = spell(TWIN, CastKind::Instant, true, effects);
        f.template.abilities.insert(s.id);
        f.data.spells.insert(s.id, s);
        let mut k = f.kernel(1);
        let mut once = true;
        script(&mut k, COMBAT_START + SimDuration(5_000), |now, k| {
            if now >= COMBAT_START
                && readiness(k, TWIN) == Some(Readiness::Now)
                && std::mem::take(&mut once)
            {
                return cast(TWIN);
            }
            Choice::Wait(Wait::NextEvent)
        });
        let trace = k.drain_trace();
        let hits: Vec<u64> = dealt(&trace, SchoolMask::PHYSICAL)
            .into_iter()
            .map(|h| h.1)
            .collect();
        let procs = |school| dealt(&trace, school).len();
        (hits, procs(SchoolMask::NATURE), procs(SchoolMask::FROST))
    };
    assert_eq!(run(off_hand), (vec![mh, oh], 1, 1));
    // With nothing in the off hand, its strike does nothing.
    assert_eq!(run(None), (vec![mh], 1, 0));
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

const CAT: AuraId = AuraId(900_201);
const BEAR: AuraId = AuraId(900_202);
const PROWL: AuraId = AuraId(900_203);
const SUBTERFUGE: AuraId = AuraId(900_204);
const CAT_FORM: SpellId = SpellId(900_211);
const BEAR_FORM: SpellId = SpellId(900_212);
const SHRED: SpellId = SpellId(900_213);
const BOLT: SpellId = SpellId(900_214);
const PROWL_SPELL: SpellId = SpellId(900_215);
const SAP: SpellId = SpellId(900_216);
const PRICK: SpellId = SpellId(900_217);
const PAWS: u32 = 1000;

fn aura(id: AuraId, modifiers: Vec<Modifier>) -> AuraDef {
    AuraDef {
        id,
        name: format!("aura {}", id.0),
        duration: None,
        max_stacks: 1,
        refresh: RefreshRule::Replace,
        periodic: None,
        value: None,
        modifiers,
        listeners: Vec::new(),
        overrides: Vec::new(),
        on_expire: Vec::new(),
        cancelable: false,
        blocked_by: None,
        form: None,
        stealth: None,
        ends_with: None,
        persists_through_death: false,
        unique_per_source: false,
        prevents_death: None,
        ground: None,
    }
}

fn spell(id: SpellId, cast: CastKind, hostile: bool, effects: Vec<Effect>) -> SpellDef {
    SpellDef {
        id,
        name: format!("spell {}", id.0),
        school: SchoolMask::PHYSICAL,
        cast,
        gcd: hostile.then_some(GcdDef {
            base: SimDuration(1000),
            hasted: false,
            floor: SimDuration(1000),
        }),
        cooldown: None,
        costs: Vec::new(),
        targeting: if hostile {
            Targeting::Enemy
        } else {
            Targeting::SelfOnly
        },
        hostile,
        range: None,
        speed: None,
        min_travel: SimDuration::ZERO,
        rolls_on_impact: false,
        castable_while_moving: false,
        usable_while_casting: false,
        weapon: None,
        requires: Vec::new(),
        effects,
    }
}

fn apply_self(aura: AuraId) -> Effect {
    Effect::ApplyAura {
        aura,
        target: EffectTarget::Caster,
        stacks: 1,
        duration: None,
        per_unit_spent: None,
    }
}

/// A druid of sorts: Cat Form (fast paws) and Bear Form share a group;
/// Shred needs Cat Form and hits twice as hard from Prowl, which needs Cat
/// Form, is cast between pulls, keeps Sap, ends with Cat Form, and leaves
/// Subterfuge when broken. Bolt is a hard cast and Prick hurts the caster;
/// Sap and Prick are castable in Cat Form.
fn druid() -> Fixture {
    let mut f = fixture(weapon(2600, 1000.0), Some(weapon(1300, 100.0)), Vec::new());
    let mut cat = aura(CAT, Vec::new());
    cat.form = Some(FormDef {
        group: 1,
        allows_all: false,
        allows: vec![PROWL_SPELL, SAP, PRICK],
        weapon: Some(weapon(PAWS, 300.0)),
    });
    let mut bear = aura(BEAR, Vec::new());
    bear.form = Some(FormDef {
        group: 1,
        allows_all: false,
        allows: Vec::new(),
        weapon: None,
    });
    let mut prowl = aura(
        PROWL,
        vec![Modifier {
            scope: ModScope::Spell(SHRED),
            ..modifier(ModKind::DamageDonePct, 100.0)
        }],
    );
    prowl.stealth = Some(StealthDef {
        keeps: vec![SAP],
        breaks_on_damage: true,
        on_break: vec![apply_self(SUBTERFUGE)],
    });
    prowl.ends_with = Some(CAT);
    let mut subterfuge = aura(SUBTERFUGE, Vec::new());
    subterfuge.duration = Some(SimDuration(3000));
    for a in [cat, bear, prowl, subterfuge] {
        f.data.auras.insert(a.id, a);
    }
    let mut shred = spell(
        SHRED,
        CastKind::Instant,
        true,
        vec![damage(1000.0, SchoolMask::PHYSICAL)],
    );
    shred.requires = vec![Requirement::AnyAura(vec![CAT])];
    let mut prowl = spell(
        PROWL_SPELL,
        CastKind::Instant,
        false,
        vec![apply_self(PROWL)],
    );
    prowl.requires = vec![Requirement::AnyAura(vec![CAT]), Requirement::OutOfCombat];
    let bolt = CastKind::Cast {
        time: SimDuration(1500),
        hasted: false,
    };
    let prick = Effect::Damage {
        amount: Coefficient::Flat(1.0),
        school: SchoolMask::PHYSICAL,
        target: EffectTarget::Caster,
        aoe: None,
        ignores_armor: false,
        hand: None,
        per_count: None,
        unmodified: false,
    };
    for s in [
        spell(CAT_FORM, CastKind::Instant, false, vec![apply_self(CAT)]),
        spell(BEAR_FORM, CastKind::Instant, false, vec![apply_self(BEAR)]),
        shred,
        spell(BOLT, bolt, true, vec![damage(1.0, SchoolMask::NATURE)]),
        prowl,
        spell(SAP, CastKind::Instant, true, Vec::new()),
        spell(PRICK, CastKind::Instant, false, vec![prick]),
    ] {
        f.template.abilities.insert(s.id);
        f.data.spells.insert(s.id, s);
    }
    f
}

/// Answer with `policy`, given the time, until `until`.
fn script(
    k: &mut Kernel<PartyMechanics>,
    until: SimTime,
    mut policy: impl FnMut(SimTime, &Kernel<PartyMechanics>) -> Choice,
) {
    loop {
        match k.advance().unwrap() {
            Step::Done(_) => return,
            Step::Decide(req) => {
                if req.now >= until {
                    return;
                }
                let choice = policy(req.now, k);
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
}

fn readiness(k: &Kernel<PartyMechanics>, ability: SpellId) -> Option<Readiness> {
    k.legal(Seat(0)).abilities.get(&ability).copied()
}

fn holds(k: &Kernel<PartyMechanics>, aura: AuraId) -> bool {
    k.state().auras(ActorId(0)).iter().any(|a| a.aura == aura)
}

fn cast(ability: SpellId) -> Choice {
    Choice::Cast {
        ability,
        target: TargetSel::Primary,
        opts: CastOpts::default(),
    }
}

/// Cast `ability` once it's ready, or wait for the next event.
fn cast_when_ready(k: &Kernel<PartyMechanics>, ability: SpellId) -> Choice {
    if readiness(k, ability) == Some(Readiness::Now) {
        cast(ability)
    } else {
        Choice::Wait(Wait::NextEvent)
    }
}

fn applied(trace: &[TraceRecord], aura: AuraId) -> usize {
    trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::AuraApplied { aura: a, .. } if a == aura))
        .count()
}

#[test]
fn forms_exclude_each_other_and_swap_weapons() {
    let f = druid();
    let mut k = f.kernel(1);
    let shift = COMBAT_START + SimDuration(10_000);
    let end = COMBAT_START + SimDuration(30_000);
    script(&mut k, end, |now, k| {
        if now < COMBAT_START && !holds(k, CAT) {
            cast(CAT_FORM)
        } else if now < shift {
            Choice::Wait(Wait::Until(shift))
        } else if !holds(k, BEAR) {
            cast(BEAR_FORM)
        } else {
            Choice::Wait(Wait::Until(end))
        }
    });
    assert!(holds(&k, BEAR) && !holds(&k, CAT));
    let trace = k.drain_trace();
    let (cat, bear): (Vec<_>, Vec<_>) = trace.into_iter().partition(|r| r.time < shift);
    // Paws alone, which never miss.
    let (paws, none) = white_hits(&cat);
    assert!(none.is_empty());
    assert_eq!(interval(&paws), PAWS);
    assert_eq!(paws.len(), 10);
    // Out of Cat Form, both weapons again.
    let (mh, oh) = white_hits(&bear);
    assert_eq!(interval(&mh), 2600);
    assert!(!oh.is_empty());
}

#[test]
fn spells_need_their_form_and_others_shift_out_of_it() {
    let f = druid();
    let mut k = f.kernel(2);
    let mut seen = Vec::new();
    let mut shredded = false;
    script(&mut k, COMBAT_START + SimDuration(10_000), |now, k| {
        if now < COMBAT_START {
            seen.push(readiness(k, SHRED));
            return if holds(k, CAT) {
                Choice::Wait(Wait::Until(COMBAT_START))
            } else {
                cast(CAT_FORM)
            };
        }
        if !shredded {
            shredded = readiness(k, SHRED) == Some(Readiness::Now);
            return cast_when_ready(k, SHRED);
        }
        if holds(k, CAT) {
            cast_when_ready(k, BOLT)
        } else {
            seen.push(readiness(k, SHRED));
            Choice::Wait(Wait::Until(COMBAT_START + SimDuration(10_000)))
        }
    });
    assert_eq!(seen.first(), Some(&Some(Readiness::Blocked)));
    assert_eq!(seen.last(), Some(&Some(Readiness::Blocked)));
    assert!(!holds(&k, CAT));
    let trace = k.drain_trace();
    assert_eq!(dealt(&trace, SchoolMask::NATURE).len(), 1);
    assert!(dealt(&trace, SchoolMask::PHYSICAL)
        .iter()
        .any(|h| h.2 == Some(SHRED)));
}

#[test]
fn stealth_holds_swings_and_breaks_after_the_opener() {
    let f = druid();
    let mut k = f.kernel(3);
    let open = COMBAT_START + SimDuration(5_000);
    let end = COMBAT_START + SimDuration(20_000);
    let mut in_combat = None;
    script(&mut k, end, |now, k| {
        if now < COMBAT_START {
            return if !holds(k, CAT) {
                cast(CAT_FORM)
            } else if !holds(k, PROWL) {
                cast(PROWL_SPELL)
            } else {
                Choice::Wait(Wait::Until(open))
            };
        }
        in_combat.get_or_insert(readiness(k, PROWL_SPELL));
        if now < open {
            Choice::Wait(Wait::Until(open))
        } else {
            cast_when_ready(k, SHRED)
        }
    });
    assert_eq!(in_combat, Some(Some(Readiness::Blocked)));
    assert!(holds(&k, CAT) && !holds(&k, PROWL));
    let trace = k.drain_trace();
    let (paws, _) = white_hits(&trace);
    assert!(paws.iter().all(|&t| t >= open), "{paws:?}");
    assert!(paws.len() >= 10);
    // The opener kept Prowl's bonus; later Shreds don't get it.
    let shreds: Vec<u64> = dealt(&trace, SchoolMask::PHYSICAL)
        .into_iter()
        .filter(|h| h.2 == Some(SHRED))
        .map(|h| h.1)
        .collect();
    assert!(shreds.len() > 2);
    let ratio = shreds[0] as f64 / shreds[1] as f64;
    assert!((ratio - 2.0).abs() < 0.01, "{shreds:?}");
    assert_eq!(applied(&trace, SUBTERFUGE), 1);
}

#[test]
fn stealth_keeps_listed_spells_and_breaks_on_damage_taken() {
    let f = druid();
    let mut k = f.kernel(4);
    let sap = COMBAT_START + SimDuration(2_000);
    let prick = COMBAT_START + SimDuration(4_000);
    let end = COMBAT_START + SimDuration(6_000);
    let mut after_sap = None;
    script(&mut k, end, |now, k| {
        if now < COMBAT_START {
            return if !holds(k, CAT) {
                cast(CAT_FORM)
            } else if !holds(k, PROWL) {
                cast(PROWL_SPELL)
            } else {
                Choice::Wait(Wait::Until(sap))
            };
        }
        if now < sap {
            Choice::Wait(Wait::Until(sap))
        } else if now < prick {
            if k.state().last_cast(Seat(0)).is_some_and(|c| c.spell == SAP) {
                after_sap.get_or_insert(holds(k, PROWL));
                Choice::Wait(Wait::Until(prick))
            } else {
                cast_when_ready(k, SAP)
            }
        } else if holds(k, PROWL) {
            cast_when_ready(k, PRICK)
        } else {
            Choice::Wait(Wait::Until(end))
        }
    });
    assert_eq!(after_sap, Some(true));
    assert!(!holds(&k, PROWL));
    let trace = k.drain_trace();
    assert_eq!(applied(&trace, SUBTERFUGE), 1);
    let (paws, _) = white_hits(&trace);
    assert!(paws.first().is_some_and(|&t| t >= prick), "{paws:?}");
}

#[test]
fn stealth_ends_with_its_form_without_breaking() {
    let f = druid();
    let mut k = f.kernel(5);
    script(&mut k, COMBAT_START, |_, k| {
        if !holds(k, CAT) && !holds(k, BEAR) {
            cast(CAT_FORM)
        } else if holds(k, CAT) && !holds(k, PROWL) {
            cast(PROWL_SPELL)
        } else if holds(k, CAT) {
            cast(BEAR_FORM)
        } else {
            Choice::Wait(Wait::Until(COMBAT_START))
        }
    });
    assert!(holds(&k, BEAR) && !holds(&k, CAT) && !holds(&k, PROWL));
    let trace = k.drain_trace();
    assert_eq!(applied(&trace, PROWL), 1);
    assert_eq!(applied(&trace, SUBTERFUGE), 0);
}

/// Also the id of the bleed it applies, so the bleed's ticks name it.
const REND: SpellId = SpellId(900_301);
const PIERCE: SpellId = SpellId(900_302);

#[test]
fn armor_reduces_direct_physical_hits_but_not_bleeds() {
    const ARMOR: f64 = 5_000.0;
    let mut f = fixture(weapon(2600, 1000.0), None, Vec::new());
    f.template.main_hand = None;
    for e in Arc::make_mut(&mut f.enemies).enemies.values_mut() {
        e.defense.armor = ARMOR;
    }
    let bleed = AuraId(REND.0);
    let mut rend_bleed = aura(bleed, Vec::new());
    rend_bleed.duration = Some(SimDuration(3000));
    rend_bleed.periodic = Some(Periodic {
        period: SimDuration(1000),
        hasted: false,
        partial_final_tick: false,
        effects: vec![damage(1000.0, SchoolMask::PHYSICAL)],
    });
    f.data.auras.insert(bleed, rend_bleed);
    let pierce = Effect::Damage {
        amount: Coefficient::Flat(1000.0),
        school: SchoolMask::PHYSICAL,
        target: EffectTarget::Target,
        aoe: None,
        ignores_armor: true,
        hand: None,
        per_count: None,
        unmodified: false,
    };
    let apply_bleed = Effect::ApplyAura {
        aura: bleed,
        target: EffectTarget::Target,
        stacks: 1,
        duration: None,
        per_unit_spent: None,
    };
    for s in [
        spell(
            STRIKE,
            CastKind::Instant,
            true,
            vec![damage(1000.0, SchoolMask::PHYSICAL)],
        ),
        spell(PIERCE, CastKind::Instant, true, vec![pierce]),
        spell(REND, CastKind::Instant, true, vec![apply_bleed]),
    ] {
        f.template.abilities.insert(s.id);
        f.data.spells.insert(s.id, s);
    }
    let mut k = f.kernel(1);
    let end = COMBAT_START + SimDuration(10_000);
    let mut order = vec![PIERCE, STRIKE, REND];
    script(&mut k, end, |now, k| match order.last() {
        Some(&next) if now >= COMBAT_START => {
            let choice = cast_when_ready(k, next);
            if matches!(choice, Choice::Cast { .. }) {
                order.pop();
            }
            choice
        }
        Some(_) => Choice::Wait(Wait::Until(COMBAT_START)),
        None => Choice::Wait(Wait::Until(end)),
    });
    let trace = k.drain_trace();
    let physical = dealt(&trace, SchoolMask::PHYSICAL);
    let by = |spell| -> Vec<u64> {
        physical
            .iter()
            .filter(|h| h.2 == Some(spell))
            .map(|h| h.1)
            .collect()
    };
    let (ticks, strike, pierced) = (by(REND), by(STRIKE), by(PIERCE));
    assert_eq!(ticks.len(), 3, "{physical:?}");
    assert!(ticks.iter().all(|&t| t == ticks[0]));
    assert_eq!(pierced, vec![ticks[0]]);
    let reduced = ticks[0] as f64 * (1.0 - ARMOR / (ARMOR + f.data.curves.armor_constant));
    assert_eq!(strike.len(), 1);
    assert!(
        (strike[0] as f64 - reduced).abs() <= 1.0,
        "{strike:?} vs {reduced}"
    );
}

#[test]
fn an_adds_own_armor_reduces_physical_hits_on_it() {
    const ARMOR: f64 = 5_000.0;
    let mut f = fixture(weapon(2600, 1000.0), None, Vec::new());
    f.template.main_hand = None;
    let enemies = Arc::make_mut(&mut f.enemies);
    let dummy = EnemyKey("target_dummy".into());
    let imp = EnemyKey("imp".into());
    let mut add = enemies.enemies[&dummy].clone();
    add.key = imp.clone();
    add.kind = EnemyKind::Add;
    add.defense.armor = ARMOR;
    enemies.enemies.insert(imp.clone(), add);
    enemies.enemies.get_mut(&dummy).unwrap().rules = vec![EnemyRule {
        name: EventName("summon".into()),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::SpawnAdds {
            adds: vec![(imp, 1)],
            distance: None,
            despawn_with_spawner: false,
        },
    }];
    let s = spell(
        STRIKE,
        CastKind::Instant,
        true,
        vec![damage(1000.0, SchoolMask::PHYSICAL)],
    );
    f.template.abilities.insert(s.id);
    f.data.spells.insert(s.id, s);
    let mut k = f.kernel(1);
    let end = COMBAT_START + SimDuration(10_000);
    let mut struck = 0;
    script(&mut k, end, |now, k| {
        let enemies = k.state().enemies().to_vec();
        if now < COMBAT_START || enemies.len() < 2 {
            return Choice::Wait(Wait::NextEvent);
        }
        if struck == 2 || readiness(k, STRIKE) != Some(Readiness::Now) {
            return Choice::Wait(Wait::NextEvent);
        }
        struck += 1;
        Choice::Cast {
            ability: STRIKE,
            target: TargetSel::Actor(enemies[struck - 1]),
            opts: CastOpts::default(),
        }
    });
    let trace = k.drain_trace();
    let state = k.state();
    let on = |target: ActorId| -> Vec<u64> {
        trace
            .iter()
            .filter_map(|r| match r.event {
                TraceEvent::Damage(d) if d.target == target && d.spell == Some(STRIKE) => {
                    Some(d.amount)
                }
                _ => None,
            })
            .collect()
    };
    let (boss, add) = (state.enemies()[0], state.enemies()[1]);
    let (full, armored) = (on(boss), on(add));
    assert_eq!((full.len(), armored.len()), (1, 1), "{trace:?}");
    let reduced = full[0] as f64 * (1.0 - ARMOR / (ARMOR + f.data.curves.armor_constant));
    assert!(
        (armored[0] as f64 - reduced).abs() <= 1.0,
        "{armored:?} vs {reduced}"
    );
}

const CHANNEL: SpellId = SpellId(900_401);
const FRENZY: AuraId = AuraId(900_402);

/// A 3 s, three-tick channel of 1000 physical damage a tick, cast
/// whenever it's ready from the pull.
fn channeler(swings: bool, tick: Vec<Effect>) -> Fixture {
    let mut f = fixture(weapon(1000, 1000.0), None, Vec::new());
    let cast = CastKind::Channel {
        duration: SimDuration(3000),
        ticks: 3,
        hasted: true,
        swings,
    };
    let s = spell(CHANNEL, cast, true, tick);
    f.template.abilities.insert(s.id);
    f.data.spells.insert(s.id, s);
    f
}

fn channel_starts(trace: &[TraceRecord]) -> Vec<SimTime> {
    trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::CastStart { spell, .. } if spell == CHANNEL))
        .map(|r| r.time)
        .collect()
}

/// Channel `times` times from the pull, as soon as each is possible.
fn play_channel(f: &Fixture, times: usize, until: SimTime) -> Vec<TraceRecord> {
    let mut k = f.kernel(1);
    let mut left = times;
    script(&mut k, until, |now, k| {
        if now < COMBAT_START {
            return Choice::Wait(Wait::Until(COMBAT_START));
        }
        let choice = if left > 0 {
            cast_when_ready(k, CHANNEL)
        } else {
            Choice::Wait(Wait::Until(until))
        };
        if matches!(choice, Choice::Cast { .. }) {
            left -= 1;
        }
        choice
    });
    k.drain_trace()
}

#[test]
fn channels_succeed_at_the_start_and_tick_as_periodic_damage() {
    let run = |armor: f64| {
        let mut f = channeler(false, vec![damage(1000.0, SchoolMask::PHYSICAL)]);
        f.template.main_hand = None;
        for e in Arc::make_mut(&mut f.enemies).enemies.values_mut() {
            e.defense.armor = armor;
        }
        f.data.auras.get_mut(&KIT).unwrap().listeners = vec![listener(
            ListenFor::CastComplete {
                spell: Some(CHANNEL),
                school: None,
            },
            ProcChance::Always,
            vec![damage(1.0, SchoolMask::ARCANE)],
        )];
        let trace = play_channel(&f, 2, COMBAT_START + SimDuration(3_001));
        let starts = channel_starts(&trace);
        let succeeded: Vec<SimTime> = dealt(&trace, SchoolMask::ARCANE)
            .into_iter()
            .map(|h| h.0)
            .collect();
        assert_eq!(succeeded, starts);
        let ticks: Vec<(SimTime, u64)> = dealt(&trace, SchoolMask::PHYSICAL)
            .into_iter()
            .filter(|h| h.2 == Some(CHANNEL) && h.0 <= starts[0] + SimDuration(3000))
            .map(|h| (h.0, h.1))
            .collect();
        (starts[0], ticks)
    };
    let (start, ticks) = run(5_000.0);
    let at = |ms| start + SimDuration(ms);
    assert_eq!(
        ticks.iter().map(|t| t.0).collect::<Vec<_>>(),
        vec![at(1000), at(2000), at(3000)]
    );
    // Armor doesn't touch a channel's own ticks.
    assert_eq!(ticks, run(0.0).1);
}

#[test]
fn a_channels_haste_is_fixed_when_it_starts() {
    let mut frenzy = aura(FRENZY, vec![modifier(ModKind::HastePct, 50.0)]);
    frenzy.duration = Some(SimDuration(60_000));
    let mut f = channeler(false, vec![apply_self(FRENZY)]);
    f.template.main_hand = None;
    f.data.auras.insert(FRENZY, frenzy);
    let trace = play_channel(&f, 2, COMBAT_START + SimDuration(6_001));
    let starts = channel_starts(&trace);
    let ticks: Vec<SimTime> = trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::ChannelTick { .. }))
        .map(|r| r.time)
        .collect();
    let t0 = starts[0];
    let at = |ms| t0 + SimDuration(ms);
    // The first tick hastes the caster by half; the rest of this channel
    // keeps its pace, the next one runs two thirds as long.
    assert_eq!(starts[..2], [t0, at(3000)]);
    assert_eq!(
        ticks[..6],
        [at(1000), at(2000), at(3000), at(3667), at(4333), at(5000)]
    );
}

const EMPOWER: SpellId = SpellId(900_403);

#[test]
fn empowers_pay_as_they_start_and_add_their_stage_on_release() {
    let mut f = fixture(weapon(2600, 1000.0), None, Vec::new());
    f.template.main_hand = None;
    f.template
        .resources
        .iter_mut()
        .find(|r| r.kind == ResourceKind::Rage)
        .unwrap()
        .initial = 100.0;
    f.data.auras.get_mut(&KIT).unwrap().listeners = vec![listener(
        ListenFor::CastComplete {
            spell: Some(EMPOWER),
            school: None,
        },
        ProcChance::Always,
        vec![damage(1.0, SchoolMask::ARCANE)],
    )];
    let cast = CastKind::Empower {
        stages: vec![SimDuration(1000), SimDuration(2000)],
        hasted: false,
        hold: SimDuration(1000),
        stage_effects: vec![
            vec![damage(10.0, SchoolMask::FIRE)],
            vec![damage(20.0, SchoolMask::FIRE)],
        ],
    };
    let mut s = spell(EMPOWER, cast, true, vec![damage(1000.0, SchoolMask::FIRE)]);
    s.costs = vec![Cost {
        kind: ResourceKind::Rage,
        amount: 30.0,
        extra: 0.0,
        scaling: SpendScaling::None,
    }];
    f.template.abilities.insert(s.id);
    f.data.spells.insert(s.id, s);

    let mut k = f.kernel(1);
    let end = COMBAT_START + SimDuration(5_000);
    let mut cast_once = true;
    let mut at_stage_one = None;
    script(&mut k, end, |now, k| {
        if now < COMBAT_START {
            return Choice::Wait(Wait::Until(COMBAT_START));
        }
        if k.state()
            .actor(ActorId(0))
            .and_then(|a| a.casting)
            .is_some()
        {
            let rage = k.state().resource(ActorId(0), ResourceKind::Rage).unwrap();
            at_stage_one.get_or_insert((now, rage.value));
            return Choice::Wait(Wait::NextEvent);
        }
        if std::mem::take(&mut cast_once) {
            return Choice::Cast {
                ability: EMPOWER,
                target: TargetSel::Primary,
                opts: CastOpts {
                    empower: Some(2),
                    tick_wakes: true,
                },
            };
        }
        Choice::Wait(Wait::Until(end))
    });
    let trace = k.drain_trace();
    let t0 = COMBAT_START;
    // Paid at the start; nothing has gone off by the first stage.
    assert_eq!(at_stage_one, Some((t0 + SimDuration(1000), 70.0)));
    let released = t0 + SimDuration(2000);
    let fire: Vec<(SimTime, u64)> = dealt(&trace, SchoolMask::FIRE)
        .into_iter()
        .map(|h| (h.0, h.1))
        .collect();
    // The base hit, then stage 2's (no crits, no versatility here).
    assert_eq!(fire, vec![(released, 1000), (released, 20)]);
    let listened: Vec<SimTime> = dealt(&trace, SchoolMask::ARCANE)
        .into_iter()
        .map(|h| h.0)
        .collect();
    assert_eq!(listened, vec![released]);
}

const POTION: SpellId = SpellId(900_501);
const ELIXIR: SpellId = SpellId(900_502);
const REFRESH: SpellId = SpellId(900_503);

#[test]
fn adjusting_one_spells_category_cooldown_adjusts_them_all() {
    let mut f = fixture(weapon(2600, 1000.0), None, Vec::new());
    f.template.main_hand = None;
    let shared = CooldownDef {
        duration: SimDuration(30_000),
        charges: 1,
        hasted: false,
        category: Some(9),
    };
    let reset = Effect::AdjustCooldown {
        spell: POTION,
        change: CooldownChange::Reset,
    };
    for (id, cooldown, effects) in [
        (POTION, Some(shared), vec![damage(1.0, SchoolMask::FIRE)]),
        (ELIXIR, Some(shared), vec![damage(1.0, SchoolMask::FROST)]),
        (REFRESH, None, vec![reset]),
    ] {
        let mut s = spell(id, CastKind::Instant, true, effects);
        s.cooldown = cooldown;
        f.template.abilities.insert(id);
        f.data.spells.insert(id, s);
    }

    let mut k = f.kernel(1);
    let end = COMBAT_START + SimDuration(5_000);
    let mut plan = [POTION, REFRESH, ELIXIR].into_iter().peekable();
    let mut elixir_before_refresh = None;
    script(&mut k, end, |now, k| {
        if now < COMBAT_START {
            return Choice::Wait(Wait::Until(COMBAT_START));
        }
        let Some(&next) = plan.peek() else {
            return Choice::Wait(Wait::Until(end));
        };
        if readiness(k, next) != Some(Readiness::Now) {
            return Choice::Wait(Wait::NextEvent);
        }
        if next == REFRESH {
            elixir_before_refresh = readiness(k, ELIXIR);
        }
        plan.next();
        cast(next)
    });
    let trace = k.drain_trace();
    // The potion put the elixir on its 30 s cooldown too.
    assert!(
        matches!(elixir_before_refresh, Some(Readiness::In(d)) if d > SimDuration(25_000)),
        "{elixir_before_refresh:?}"
    );
    let at = |school| -> Vec<SimTime> { dealt(&trace, school).into_iter().map(|h| h.0).collect() };
    let t0 = COMBAT_START;
    // Resetting the potion freed the elixir one GCD later.
    assert_eq!(at(SchoolMask::FIRE), vec![t0]);
    assert_eq!(at(SchoolMask::FROST), vec![t0 + SimDuration(2000)]);
}

#[test]
fn swings_during_a_channel_do_nothing_unless_it_allows_them() {
    const SPEED: u32 = 1300;
    let end = COMBAT_START + SimDuration(9_001);
    let hits = |swings: bool| {
        let mut f = channeler(swings, Vec::new());
        f.template.main_hand = Some(weapon(SPEED, 1000.0));
        let trace = play_channel(&f, 1, end);
        let start = channel_starts(&trace)[0];
        let (hits, _) = white_hits(&trace);
        let (during, outside): (Vec<SimTime>, Vec<SimTime>) = hits
            .into_iter()
            .partition(|&t| t > start && t < start + SimDuration(3000));
        (during, outside)
    };
    let (during, outside) = hits(false);
    assert!(during.is_empty(), "{during:?}");
    // The swings it ate kept the timer's rhythm: none waited for its end.
    assert!(outside.len() >= 4, "{outside:?}");
    assert!(
        outside
            .windows(2)
            .all(|w| (w[1] - w[0]).millis() % SPEED == 0),
        "{outside:?}"
    );
    let (during, _) = hits(true);
    assert_eq!(during.len(), 2, "{during:?}");
}

const COUNTER: AuraId = AuraId(900_601);
const CAPPED: AuraId = AuraId(900_602);
const FILL: SpellId = SpellId(900_603);
const SMALL_WARD: AuraId = AuraId(900_611);
const BIG_WARD: AuraId = AuraId(900_612);
const WARD: SpellId = SpellId(900_613);
const FIREBOLT: SpellId = SpellId(900_614);
const SMASH: SpellId = SpellId(900_615);
const SEED: AuraId = AuraId(900_621);
const SOW: SpellId = SpellId(900_622);
const BOSS_SHIELD: AuraId = AuraId(900_631);

fn valued(id: AuraId, value: AuraValue) -> AuraDef {
    AuraDef {
        value: Some(value),
        ..aura(id, Vec::new())
    }
}

fn counter() -> AuraValue {
    AuraValue {
        kind: AuraValueKind::Counter,
        initial: None,
        cap: None,
        threshold: None,
        on_threshold: Vec::new(),
    }
}

fn shield(school: SchoolMask, initial: Coefficient) -> AuraValue {
    AuraValue {
        kind: AuraValueKind::Absorb { school },
        initial: Some(initial),
        ..counter()
    }
}

fn add_value(aura: AuraId, target: EffectTarget, amount: Coefficient) -> Effect {
    Effect::AddAuraValue {
        aura,
        target,
        amount,
    }
}

fn hit_all(amount: f64, school: SchoolMask) -> Effect {
    Effect::Damage {
        amount: Coefficient::Flat(amount),
        school,
        target: EffectTarget::AllEnemies,
        aoe: None,
        ignores_armor: false,
        hand: None,
        per_count: None,
        unmodified: false,
    }
}

fn apply_to_target(aura: AuraId) -> Effect {
    Effect::ApplyAura {
        aura,
        target: EffectTarget::Target,
        stacks: 1,
        duration: None,
        per_unit_spent: None,
    }
}

/// A caster with no auto-attacks, at a dummy without armor.
fn caster(kit: Vec<Listener>) -> Fixture {
    let mut f = fixture(weapon(2600, 1000.0), None, kit);
    f.template.melee = false;
    for e in Arc::make_mut(&mut f.enemies).enemies.values_mut() {
        e.defense.armor = 0.0;
    }
    f.sampler = Sampler::new(f.sampler.spec().clone(), Arc::clone(&f.enemies)).unwrap();
    f
}

fn learn(f: &mut Fixture, s: SpellDef) {
    f.template.abilities.insert(s.id);
    f.data.spells.insert(s.id, s);
}

/// Cast `order` from the pull, each once it's ready.
fn cast_in_order(
    f: &Fixture,
    order: &[SpellId],
    until: SimTime,
) -> (Kernel<PartyMechanics>, Vec<TraceRecord>) {
    let mut k = f.kernel(1);
    let mut next = order.iter().copied().peekable();
    script(&mut k, until, |now, k| {
        if now < COMBAT_START {
            return Choice::Wait(Wait::Until(COMBAT_START));
        }
        match next.peek() {
            Some(&s) if readiness(k, s) == Some(Readiness::Now) => {
                next.next();
                cast(s)
            }
            Some(_) => Choice::Wait(Wait::NextEvent),
            None => Choice::Wait(Wait::Until(until)),
        }
    });
    let trace = k.drain_trace();
    (k, trace)
}

fn value_of(k: &Kernel<PartyMechanics>, holder: ActorId, aura: AuraId) -> Option<f64> {
    k.state()
        .auras(holder)
        .iter()
        .find(|i| i.aura == aura)
        .map(|i| i.value)
}

#[test]
fn thresholds_fire_once_per_crossing_and_caps_clamp() {
    let mut f = caster(Vec::new());
    let mut boom = counter();
    boom.threshold = Some(Coefficient::Flat(100.0));
    boom.cap = Some(Coefficient::Flat(1000.0));
    boom.on_threshold = vec![hit_all(1.0, SchoolMask::FIRE)];
    let mut capped = counter();
    capped.cap = Some(Coefficient::Flat(120.0));
    f.data.auras.insert(COUNTER, valued(COUNTER, boom));
    f.data.auras.insert(CAPPED, valued(CAPPED, capped));
    let fill = vec![
        add_value(COUNTER, EffectTarget::Caster, Coefficient::Flat(250.0)),
        add_value(CAPPED, EffectTarget::Caster, Coefficient::Flat(250.0)),
    ];
    learn(&mut f, spell(FILL, CastKind::Instant, true, fill));

    let (k, trace) = cast_in_order(&f, &[FILL], COMBAT_START + SimDuration(2_000));
    let booms: Vec<SimTime> = dealt(&trace, SchoolMask::FIRE)
        .iter()
        .map(|h| h.0)
        .collect();
    assert_eq!(booms, vec![COMBAT_START; 2]);
    assert_eq!(value_of(&k, ActorId(0), COUNTER), Some(50.0));
    assert_eq!(value_of(&k, ActorId(0), CAPPED), Some(120.0));
}

#[test]
fn absorbs_soak_their_school_smallest_first_and_go_when_spent() {
    let kit = vec![listener(
        ListenFor::DamageDealt {
            spell: Some(FIREBOLT),
            school: None,
            crit_only: false,
            killing_blow: false,
        },
        ProcChance::Always,
        vec![damage(1.0, SchoolMask::ARCANE)],
    )];
    let mut f = caster(kit);
    let fire = SchoolMask::FIRE;
    f.data.auras.insert(
        SMALL_WARD,
        valued(SMALL_WARD, shield(fire, Coefficient::Flat(200.0))),
    );
    f.data.auras.insert(
        BIG_WARD,
        valued(BIG_WARD, shield(fire, Coefficient::Flat(500.0))),
    );
    let ward = vec![apply_to_target(BIG_WARD), apply_to_target(SMALL_WARD)];
    learn(&mut f, spell(WARD, CastKind::Instant, true, ward));
    learn(
        &mut f,
        spell(FIREBOLT, CastKind::Instant, true, vec![damage(300.0, fire)]),
    );
    let smash = vec![damage(300.0, SchoolMask::PHYSICAL)];
    learn(&mut f, spell(SMASH, CastKind::Instant, true, smash));
    let end = COMBAT_START + SimDuration(10_000);

    let (_, plain) = cast_in_order(&f, &[FIREBOLT], end);
    let bolt = dealt(&plain, fire)[0].1;
    assert!((234..700).contains(&bolt), "{bolt}");

    let order = [WARD, FIREBOLT, SMASH, FIREBOLT, FIREBOLT];
    let (k, trace) = cast_in_order(&f, &order, end);
    let soaked: Vec<(AuraId, u64)> = trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Absorbed { aura, amount, .. } => Some((aura, amount)),
            _ => None,
        })
        .collect();
    // The smaller shield goes first; the physical hit gets past both.
    assert_eq!(soaked[..2], [(SMALL_WARD, 200), (BIG_WARD, bolt - 200)]);
    assert_eq!(soaked.iter().map(|s| s.1).sum::<u64>(), 700);
    let burned: u64 = dealt(&trace, fire).iter().map(|h| h.1).sum();
    assert_eq!(burned, 3 * bolt - 700);
    assert_eq!(dealt(&trace, SchoolMask::PHYSICAL).len(), 1);
    let target = k.state().target(ActorId(0)).unwrap();
    assert!(value_of(&k, target, SMALL_WARD).is_none());
    assert!(value_of(&k, target, BIG_WARD).is_none());
    // A bolt the shields swallowed whole still procs.
    assert_eq!(dealt(&trace, SchoolMask::ARCANE).len(), 3);
}

/// Seed of Corruption: the debuff banks the damage its holder takes, and
/// at the threshold it bursts and goes.
#[test]
fn a_seed_bursts_once_its_holder_has_taken_enough() {
    let mut f = caster(Vec::new());
    let mut seed = counter();
    seed.threshold = Some(Coefficient::Flat(1000.0));
    seed.on_threshold = vec![
        Effect::RemoveAura {
            aura: SEED,
            target: EffectTarget::Target,
        },
        hit_all(5000.0, SchoolMask::SHADOW),
    ];
    let mut def = valued(SEED, seed);
    def.listeners = vec![listener(
        ListenFor::DamageTaken,
        ProcChance::Always,
        vec![add_value(
            SEED,
            EffectTarget::Caster,
            Coefficient::EventAmount(1.0),
        )],
    )];
    f.data.auras.insert(SEED, def);
    learn(
        &mut f,
        spell(SOW, CastKind::Instant, true, vec![apply_to_target(SEED)]),
    );
    let bolt = vec![damage(300.0, SchoolMask::FIRE)];
    learn(&mut f, spell(FIREBOLT, CastKind::Instant, true, bolt));

    let order = [SOW, FIREBOLT, FIREBOLT, FIREBOLT, FIREBOLT, FIREBOLT];
    let (k, trace) = cast_in_order(&f, &order, COMBAT_START + SimDuration(10_000));
    let bolts = dealt(&trace, SchoolMask::FIRE);
    let mut taken = 0;
    let crossed = bolts
        .iter()
        .find(|h| {
            taken += h.1;
            taken >= 1000
        })
        .expect("enough bolts")
        .0;
    let bursts: Vec<SimTime> = dealt(&trace, SchoolMask::SHADOW)
        .iter()
        .map(|h| h.0)
        .collect();
    assert_eq!(bursts, vec![crossed]);
    let target = k.state().target(ActorId(0)).unwrap();
    assert!(value_of(&k, target, SEED).is_none());
}

/// An enemy that shields itself for a tenth of its health takes exactly
/// that much more damage to kill.
#[test]
fn a_boss_shield_lengthens_the_kill_by_its_size() {
    let run = |shielded: bool| {
        let mut f = caster(Vec::new());
        let enemies = Arc::make_mut(&mut f.enemies);
        for e in enemies.enemies.values_mut() {
            e.health = 20_000;
            if shielded {
                e.rules = vec![EnemyRule {
                    name: EventName("ward".into()),
                    phase: None,
                    when: Trigger::<EnemySubject>::Elapsed(SimDuration::ZERO),
                    repeat: None,
                    action: EnemyAction::SelfAura(BOSS_SHIELD),
                }];
            }
        }
        f.sampler = Sampler::new(f.sampler.spec().clone(), Arc::clone(&f.enemies)).unwrap();
        let ward = shield(SchoolMask(u8::MAX), Coefficient::PctMaxHealth(10.0));
        f.data.auras.insert(BOSS_SHIELD, valued(BOSS_SHIELD, ward));
        let slow = CastKind::Cast {
            time: SimDuration(1000),
            hasted: false,
        };
        learn(
            &mut f,
            spell(FIREBOLT, slow, true, vec![damage(300.0, SchoolMask::FIRE)]),
        );
        let mut k = f.kernel(1);
        for _ in 0..100_000 {
            match k.advance().unwrap() {
                Step::Done(o) => return (o, k.drain_trace()),
                Step::Decide(req) => {
                    let choice = cast_when_ready(&k, FIREBOLT);
                    k.submit(req.seat, choice).unwrap();
                }
            }
        }
        panic!("the dummy should die");
    };
    let (plain, plain_trace) = run(false);
    let (shielded, _) = run(true);
    let bolt = dealt(&plain_trace, SchoolMask::FIRE)[0].1;
    let (p, s) = (&plain.seats[0], &shielded.seats[0]);
    assert_eq!((p.damage_done, p.damage_absorbed), (20_000, 0));
    assert_eq!((s.damage_done, s.damage_absorbed), (20_000, 2_000));
    assert_eq!(p.casts, 20_000_u64.div_ceil(bolt) as u32);
    assert_eq!(s.casts, 22_000_u64.div_ceil(bolt) as u32);
}

const IGNITE: AuraId = AuraId(900_641);
const KINDLE: SpellId = SpellId(900_642);

/// A bank that `draw`s each second for `duration` ms, dealing what it
/// draws to its holder as shadow damage.
fn bank(draw: BankDraw, duration: u32, partial_final_tick: bool) -> AuraDef {
    let pay = Effect::Damage {
        amount: Coefficient::EventAmount(1.0),
        school: SchoolMask::SHADOW,
        target: EffectTarget::Target,
        aoe: None,
        ignores_armor: false,
        hand: None,
        per_count: None,
        unmodified: false,
    };
    let value = AuraValue {
        kind: AuraValueKind::Bank(draw),
        ..counter()
    };
    AuraDef {
        duration: Some(SimDuration(duration)),
        periodic: Some(Periodic {
            period: SimDuration(1000),
            hasted: false,
            partial_final_tick,
            effects: vec![pay],
        }),
        ..valued(IGNITE, value)
    }
}

/// A caster whose one spell (re)applies `bank` to its target and adds
/// `amount` to it.
fn banker(bank: AuraDef, amount: f64) -> Fixture {
    let mut f = caster(Vec::new());
    f.data.auras.insert(IGNITE, bank);
    let fill = vec![
        apply_to_target(IGNITE),
        add_value(IGNITE, EffectTarget::Target, Coefficient::Flat(amount)),
    ];
    learn(&mut f, spell(KINDLE, CastKind::Instant, true, fill));
    f
}

/// Cast the banker's spell `at` these ms after the pull: the bank's
/// payouts as `(ms after the pull, amount)`.
fn payouts(f: &Fixture, at: &[u32]) -> Vec<(u32, u64)> {
    let until = COMBAT_START + SimDuration(15_000);
    let mut k = f.kernel(1);
    let mut next = at
        .iter()
        .map(|&ms| COMBAT_START + SimDuration(ms))
        .peekable();
    script(&mut k, until, |now, _| match next.peek() {
        Some(&t) if now >= t => {
            next.next();
            cast(KINDLE)
        }
        Some(&t) => Choice::Wait(Wait::Until(t)),
        None => Choice::Wait(Wait::Until(until)),
    });
    let target = k.state().target(ActorId(0)).unwrap();
    assert!(value_of(&k, target, IGNITE).is_none(), "the bank lapses");
    dealt(&k.drain_trace(), SchoolMask::SHADOW)
        .iter()
        .map(|h| (h.0.saturating_since(COMBAT_START).millis(), h.1))
        .collect()
}

fn about(amount: u64, expected: f64) -> bool {
    (amount as f64 - expected).abs() <= 2.0
}

/// Ignite, as SimC's `residual_action`: the bank pays out evenly over the
/// ticks left, and a refresh rolls what's left of it, with what was
/// added, over the new duration.
#[test]
fn a_spread_bank_pays_evenly_and_a_refresh_rolls_it_over() {
    let f = banker(bank(BankDraw::SpreadOverRemaining, 4000, false), 4000.0);
    let once = payouts(&f, &[0]);
    let unit = once[0].1;
    assert!(unit > 100, "{once:?}");
    assert_eq!(once, [1000, 2000, 3000, 4000].map(|t| (t, unit)));

    // 2000 left at 2.5s plus 4000 new, over the four whole ticks before
    // the refreshed expiry at 6.5s (no partial final tick).
    let twice = payouts(&f, &[0, 2500]);
    let times: Vec<u32> = twice.iter().map(|p| p.0).collect();
    assert_eq!(times, [1000, 2000, 3000, 4000, 5000, 6000]);
    assert_eq!(twice[..2], once[..2]);
    let expected = 1.5 * unit as f64;
    assert!(twice[2..].iter().all(|p| about(p.1, expected)), "{twice:?}");
}

/// A partial final tick counts as part of a tick when spreading, and
/// empties the bank.
#[test]
fn a_partial_final_tick_draws_the_rest_of_the_bank() {
    let f = banker(bank(BankDraw::SpreadOverRemaining, 3500, true), 3500.0);
    let paid = payouts(&f, &[0]);
    let unit = paid[0].1;
    assert_eq!(paid[..3], [1000, 2000, 3000].map(|t| (t, unit)));
    assert_eq!(paid.len(), 4, "{paid:?}");
    assert_eq!(paid[3].0, 3500);
    assert!(about(paid[3].1, unit as f64 / 2.0), "{paid:?}");
}

/// Stagger-style: each tick takes a fixed share of what's left; whatever
/// remains at expiry is lost. A cap bounds what the bank can hold.
#[test]
fn a_fractional_bank_draws_a_share_of_what_is_left_up_to_its_cap() {
    let halves = |cap: Option<f64>| {
        let mut b = bank(BankDraw::Fraction(0.5), 3000, false);
        if let Some(v) = b.value.as_mut() {
            v.cap = cap.map(Coefficient::Flat);
        }
        payouts(&banker(b, 8000.0), &[0])
    };
    let paid = halves(None);
    let times: Vec<u32> = paid.iter().map(|p| p.0).collect();
    assert_eq!(times, [1000, 2000, 3000]);
    let unit = paid[2].1 as f64;
    assert!(about(paid[0].1, 4.0 * unit), "{paid:?}");
    assert!(about(paid[1].1, 2.0 * unit), "{paid:?}");
    let capped = halves(Some(4000.0));
    assert!(about(capped[0].1, 2.0 * unit), "{capped:?}");
}

const PROBE: SpellId = SpellId(900_701);
const STACKER: AuraId = AuraId(900_702);
const STACK_TARGET: SpellId = SpellId(900_703);
const STACK_SELF: SpellId = SpellId(900_704);
const HURT_SELF: SpellId = SpellId(900_705);
const OTHER: SpellId = SpellId(900_706);
const CONDITIONAL: AuraId = AuraId(900_707);
const TALLY: AuraId = AuraId(900_708);
const TALLY_UP: SpellId = SpellId(900_709);

/// A caster whose probe spell tests `p` two ways: `Effect::If` adds an
/// arcane hit, and a passive modifier conditioned on `p` doubles its fire
/// hit (its frost hit is the control). The other spells set up state.
fn probing(p: Predicate) -> Fixture {
    let mut f = caster(Vec::new());
    let doubled = Modifier {
        scope: ModScope::School(SchoolMask::FIRE),
        condition: Some(p),
        ..modifier(ModKind::DamageDonePct, 100.0)
    };
    f.data
        .auras
        .insert(CONDITIONAL, aura(CONDITIONAL, vec![doubled]));
    f.template.passive_auras.push(CONDITIONAL);
    let mut stacker = aura(STACKER, Vec::new());
    stacker.max_stacks = 3;
    f.data.auras.insert(STACKER, stacker);
    f.data.auras.insert(TALLY, valued(TALLY, counter()));
    let probe = vec![
        Effect::If {
            when: p,
            then: vec![damage(1000.0, SchoolMask::ARCANE)],
            otherwise: Vec::new(),
        },
        damage(1000.0, SchoolMask::FIRE),
        damage(1000.0, SchoolMask::FROST),
    ];
    let stack_self = Effect::ApplyAura {
        aura: STACKER,
        target: EffectTarget::Caster,
        stacks: 1,
        duration: None,
        per_unit_spent: None,
    };
    let hurt_self = Effect::Damage {
        amount: Coefficient::Flat(1000.0),
        school: SchoolMask::HOLY,
        target: EffectTarget::Caster,
        aoe: None,
        ignores_armor: true,
        hand: None,
        per_count: None,
        unmodified: false,
    };
    let tally = add_value(TALLY, EffectTarget::Caster, Coefficient::Flat(250.0));
    for (id, effects) in [
        (PROBE, probe),
        (STACK_TARGET, vec![apply_to_target(STACKER)]),
        (STACK_SELF, vec![stack_self]),
        (HURT_SELF, vec![hurt_self]),
        (OTHER, vec![damage(1.0, SchoolMask::NATURE)]),
        (TALLY_UP, vec![tally]),
    ] {
        learn(&mut f, spell(id, CastKind::Instant, true, effects));
    }
    f
}

/// Cast `before`, then the probe: whether the predicate held for the
/// probe's last cast. The `Effect::If` and the modifier must agree.
fn probe_holds(f: &Fixture, before: &[SpellId]) -> bool {
    let order: Vec<SpellId> = before.iter().copied().chain([PROBE]).collect();
    let (_, trace) = cast_in_order(f, &order, COMBAT_START + SimDuration(30_000));
    let fire = *dealt(&trace, SchoolMask::FIRE)
        .last()
        .expect("the probe went off");
    let frost = *dealt(&trace, SchoolMask::FROST)
        .last()
        .expect("the probe went off");
    let by_if = dealt(&trace, SchoolMask::ARCANE)
        .iter()
        .any(|h| h.0 == fire.0);
    let by_modifier = if about(fire.1, 2.0 * frost.1 as f64) {
        true
    } else {
        assert!(about(fire.1, frost.1 as f64), "{fire:?} {frost:?}");
        false
    };
    assert_eq!(by_if, by_modifier, "Effect::If and the modifier disagree");
    by_if
}

#[test]
fn health_predicates_compare_fractions_of_max_health() {
    let check = |p: Predicate, before: &[SpellId], expected: bool| {
        assert_eq!(
            probe_holds(&probing(p), before),
            expected,
            "{p:?} after {before:?}"
        );
    };
    check(Predicate::TargetHpAbove(0.5), &[], true);
    check(Predicate::TargetHpAbove(1.0), &[], false);
    check(Predicate::CasterHpAbove(0.5), &[], true);
    check(Predicate::CasterHpAbove(1.0), &[], false);
    check(Predicate::CasterHpBelow(1.0), &[], false);
    check(Predicate::CasterHpBelow(1.0), &[HURT_SELF], true);
}

/// Touch of Death: the target has no more health than the caster's max.
#[test]
fn touch_of_death_compares_target_health_with_caster_max_health() {
    let at = |extra: u64| {
        let mut f = probing(Predicate::TargetHpBelowCasterMaxHp);
        let k = f.kernel(1);
        let max = k.state().actor(ActorId(0)).unwrap().max_health;
        assert!(max > 10_000, "{max}");
        for e in Arc::make_mut(&mut f.enemies).enemies.values_mut() {
            e.health = max + extra;
        }
        f.sampler = Sampler::new(f.sampler.spec().clone(), Arc::clone(&f.enemies)).unwrap();
        probe_holds(&f, &[])
    };
    assert!(at(0));
    assert!(!at(1));
}

#[test]
fn stack_and_value_predicates_count_the_right_instance() {
    let check = |p: Predicate, before: &[SpellId], expected: bool| {
        assert_eq!(
            probe_holds(&probing(p), before),
            expected,
            "{p:?} after {before:?}"
        );
    };
    let on_target = Predicate::TargetStacksAtLeast {
        aura: STACKER,
        stacks: 2,
        from_self: true,
    };
    check(on_target, &[STACK_TARGET], false);
    check(on_target, &[STACK_TARGET, STACK_TARGET], true);
    check(on_target, &[STACK_SELF, STACK_SELF], false);
    let on_self = Predicate::CasterStacksAtLeast {
        aura: STACKER,
        stacks: 2,
    };
    check(on_self, &[STACK_SELF], false);
    check(on_self, &[STACK_SELF, STACK_SELF], true);
    check(on_self, &[STACK_TARGET, STACK_TARGET], false);
    let tally = |value| Predicate::AuraValueAtLeast { aura: TALLY, value };
    check(tally(250.0), &[], false);
    check(tally(250.0), &[TALLY_UP], true);
    check(tally(251.0), &[TALLY_UP], false);
}

/// History predicates see the casts before the one whose effects are
/// running, as SimC's combo strikes do.
#[test]
fn cast_history_predicates_see_the_casts_before_this_one() {
    let check = |p: Predicate, before: &[SpellId], expected: bool| {
        assert_eq!(
            probe_holds(&probing(p), before),
            expected,
            "{p:?} after {before:?}"
        );
    };
    let twice = Predicate::RecentCasts {
        spell: OTHER,
        count: 2,
    };
    check(twice, &[OTHER, OTHER], true);
    check(twice, &[OTHER], false);
    check(twice, &[HURT_SELF, OTHER], false);
    check(twice, &[OTHER, OTHER, HURT_SELF], false);
    check(twice, &[HURT_SELF, OTHER, OTHER], true);
    let again = Predicate::RecentCasts {
        spell: PROBE,
        count: 1,
    };
    check(again, &[], false);
    check(again, &[PROBE], true);
    check(Predicate::DiffersFromLastCast, &[], true);
    check(Predicate::DiffersFromLastCast, &[OTHER], true);
    check(Predicate::DiffersFromLastCast, &[PROBE], false);
}

const RUPTURE: SpellId = SpellId(900_711);
const RUPTURE_DOT: AuraId = AuraId(900_711);

/// SimC's Rupture: its base duration (4 s) once plus once per combo point.
/// Its cost spends every point up to five but doesn't scale its damage.
fn rupture_lasts(combo_points: f64, per_unit: Option<SimDuration>) -> SimDuration {
    let mut f = caster(Vec::new());
    f.template.resources.push(ResourceDef {
        kind: ResourceKind::ComboPoints,
        max: 5.0,
        initial: combo_points,
        regen_per_sec: 0.0,
        regen_hasted: false,
        recharge: None,
        out_of_combat: None,
    });
    f.data.auras.insert(
        RUPTURE_DOT,
        AuraDef {
            duration: Some(SimDuration(4000)),
            ..aura(RUPTURE_DOT, Vec::new())
        },
    );
    let apply = Effect::ApplyAura {
        aura: RUPTURE_DOT,
        target: EffectTarget::Target,
        stacks: 1,
        duration: None,
        per_unit_spent: per_unit,
    };
    let mut rupture = spell(RUPTURE, CastKind::Instant, true, vec![apply]);
    rupture.costs = vec![Cost {
        kind: ResourceKind::ComboPoints,
        amount: 1.0,
        extra: 4.0,
        scaling: SpendScaling::None,
    }];
    learn(&mut f, rupture);
    let (k, trace) = cast_in_order(&f, &[RUPTURE], COMBAT_START + SimDuration(1000));
    let at = trace
        .iter()
        .find(|r| matches!(r.event, TraceEvent::AuraApplied { aura, .. } if aura == RUPTURE_DOT))
        .expect("Rupture applied")
        .time;
    let me = k.state().seats()[0];
    let target = k.state().target(me).unwrap();
    let dot = k
        .state()
        .auras(target)
        .iter()
        .find(|i| i.aura == RUPTURE_DOT)
        .copied();
    dot.and_then(|i| i.expires)
        .expect("Rupture up")
        .saturating_since(at)
}

#[test]
fn durations_grow_with_the_resource_spent() {
    let four = Some(SimDuration(4000));
    assert_eq!(rupture_lasts(3.0, four), SimDuration(16_000));
    assert_eq!(rupture_lasts(5.0, four), SimDuration(24_000));
    assert_eq!(rupture_lasts(5.0, None), SimDuration(4000));
}
