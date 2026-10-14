//! The checked-in Arms data (SimC's reference build, its spec tree alone
//! and then with Slayer) through the real mechanics and kernel.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::mechanics::{whole_points, HitKind};
use portunus_engine::state::AuraInstance;
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, MoveGoal, Outcome, Readiness, RunSetup,
    SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::effect::{Coefficient, ProcChance};
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::{ResourceKind, SchoolMask};
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{CombatMath, EffectCtx, IncomingHit, Kits, Outgoing, PartyMechanics};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

const MORTAL_STRIKE: SpellId = SpellId(12294);
const OVERPOWER: SpellId = SpellId(7384);
const EXECUTE: SpellId = SpellId(163201);
const EXECUTE_MASSACRE: SpellId = SpellId(281000);
const EXECUTE_FREE: SpellId = SpellId(260798);
const SLAM: SpellId = SpellId(1464);
const HEROIC_STRIKE: SpellId = SpellId(1269383);
const COLOSSUS_SMASH: SpellId = SpellId(167105);
const BLADESTORM: SpellId = SpellId(227847);
const BLADESTORM_HIT: SpellId = SpellId(50622);
const CLEAVE: SpellId = SpellId(845);
const CHARGE: SpellId = SpellId(100);
const WRECKING_THROW: SpellId = SpellId(384110);
const DEEP_WOUNDS: SpellId = SpellId(262115);
const FATALITY: SpellId = SpellId(383706);
const REND_BLEED: SpellId = SpellId(388539);

const SUDDEN_DEATH: AuraId = AuraId(52437);
const COLOSSUS_SMASH_DEBUFF: AuraId = AuraId(208086);
const REND_DOT: AuraId = AuraId(388539);
const DEEP_WOUNDS_DOT: AuraId = AuraId(262115);
const FATAL_MARK: AuraId = AuraId(383704);
const EXECUTIONERS_PRECISION: AuraId = AuraId(386633);
const WARFARE_PROC: AuraId = AuraId(1269391);
const WARFARE_STACK: AuraId = AuraId(1269394);
const HEROIC_MIGHT_ACC: AuraId = AuraId(1292058);
const HEROIC_MIGHT: AuraId = AuraId(910101);
const BLOODSURGE: AuraId = AuraId(384361);

const SLAYERS_STRIKE: SpellId = SpellId(445579);
const REAP_THE_STORM: SpellId = SpellId(446005);
const UNHINGED_MS: SpellId = SpellId(910102);
const EXECUTIONER: AuraId = AuraId(445584);
const IMMINENT_DEMISE: AuraId = AuraId(445606);
const OVERWHELMED: AuraId = AuraId(445836);
const FIERCE_FOLLOWTHROUGH: AuraId = AuraId(458689);
const VIOLENT_EUPHORIA: AuraId = AuraId(1270731);
const SUDDEN_DEATH_TALENT: AuraId = AuraId(29725);
const SLAYERS_DOMINANCE: AuraId = AuraId(444767);
const SHOW_NO_MERCY: AuraId = AuraId(444771);
const CULLING_CYCLONE: AuraId = AuraId(444778);
const DEADLY_FOCUS: AuraId = AuraId(1270718);
/// The hits Slayer's Dominance rolls on.
const SLAYER_SOURCES: [SpellId; 9] = [
    MORTAL_STRIKE,
    UNHINGED_MS,
    SLAM,
    HEROIC_STRIKE,
    CLEAVE,
    OVERPOWER,
    EXECUTE,
    EXECUTE_MASSACRE,
    EXECUTE_FREE,
];

/// The spec aura and the two talents that give autos rage.
const AUTO_RAGE: [AuraId; 3] = [AuraId(137049), AuraId(279423), AuraId(262231)];

struct Fixture {
    data: GameData,
    enemies: EnemyData,
    template: ActorTemplate,
    spec: ScenarioSpec,
}

/// The reference build's spec tree on a dummy of exactly the listed
/// health.
fn fixture() -> Fixture {
    fixture_with("arms.ron")
}

/// The whole reference build, Slayer and all, on a dummy that lasts.
fn slayer() -> Fixture {
    let mut f = fixture_with("arms_slayer.ron");
    f.dummy_health(30_000_000);
    f
}

fn fixture_with(loadout: &str) -> Fixture {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    let enemies: EnemyData = EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    let loadout: Loadout = read_ron(&dir.join("loadouts").join(loadout)).unwrap();
    let template = Compiler.compile(&data, &loadout).unwrap();
    let mut spec: ScenarioSpec = read_ron(&dir.join("scenarios/target_dummy.ron")).unwrap();
    spec.pulls[0].health = Dist::Fixed(1.0);
    Fixture {
        data,
        enemies,
        template,
        spec,
    }
}

impl Fixture {
    fn setup(&self, seed: u64) -> RunSetup {
        let enemies = Arc::new(self.enemies.clone());
        let sampler = Sampler::new(self.spec.clone(), Arc::clone(&enemies)).unwrap();
        RunSetup {
            run: Arc::new(sampler.sample(Seed(seed))),
            data: Arc::new(self.data.clone()),
            enemies,
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

    fn mechanics(&self) -> PartyMechanics {
        PartyMechanics::new(&self.setup(0), Arc::new(Kits::standard())).unwrap()
    }

    /// Autos and Bloodsurge give nothing, and rage starts at `rage` and
    /// stays there out of combat.
    fn rage_only_from_casts(&mut self, rage: f64) {
        for a in AUTO_RAGE {
            self.data.auras.get_mut(&a).unwrap().listeners.clear();
        }
        self.data.auras.get_mut(&BLOODSURGE).unwrap().periodic = None;
        let r = self
            .template
            .resources
            .iter_mut()
            .find(|r| r.kind == ResourceKind::Rage)
            .unwrap();
        r.initial = rage;
        r.out_of_combat = None;
    }

    /// The talent's aura gone from the build.
    fn without(&mut self, aura: AuraId) {
        self.template.passive_auras.retain(|&a| a != aura);
    }

    fn dummy_health(&mut self, health: u64) {
        for e in self.enemies.enemies.values_mut() {
            e.health = health;
        }
    }
}

const SEAT: Seat = Seat(0);

fn me(k: &Kernel<PartyMechanics>) -> ActorId {
    k.state().seats()[0]
}

fn boss(k: &Kernel<PartyMechanics>) -> ActorId {
    k.state().enemies()[0]
}

fn rage(k: &Kernel<PartyMechanics>) -> f64 {
    k.state().resource(me(k), ResourceKind::Rage).unwrap().value
}

fn aura_on(k: &Kernel<PartyMechanics>, holder: ActorId, aura: AuraId) -> u8 {
    k.state()
        .auras(holder)
        .iter()
        .find(|a| a.aura == aura)
        .map_or(0, |a| a.stacks)
}

fn buff(k: &Kernel<PartyMechanics>, aura: AuraId) -> u8 {
    aura_on(k, me(k), aura)
}

fn debuff(k: &Kernel<PartyMechanics>, aura: AuraId) -> u8 {
    aura_on(k, boss(k), aura)
}

fn health_pct(k: &Kernel<PartyMechanics>) -> f64 {
    let a = k.state().actor(boss(k)).unwrap();
    a.health as f64 / a.max_health as f64
}

fn ready(k: &Kernel<PartyMechanics>, ability: SpellId) -> bool {
    k.legal(SEAT).abilities.get(&ability) == Some(&Readiness::Now)
}

fn cast(ability: SpellId) -> Choice {
    Choice::Cast {
        ability,
        target: TargetSel::Primary,
        opts: CastOpts::default(),
    }
}

/// Into melee range, then nothing.
fn idle(k: &Kernel<PartyMechanics>) -> Choice {
    let state = k.state();
    let Some(&target) = state.enemies().first() else {
        return Choice::Wait(Wait::NextEvent);
    };
    if state
        .distance(SEAT, target)
        .is_some_and(|d| d > MELEE_RANGE)
        && state.movement(SEAT).is_none()
        && k.legal(SEAT).can_move
    {
        return Choice::Move(MoveGoal::Approach {
            target,
            within: MELEE_RANGE,
        });
    }
    Choice::Wait(Wait::NextEvent)
}

/// Into melee, then the first of `order` that's ready.
fn first_of(order: &'static [SpellId]) -> impl Fn(&Kernel<PartyMechanics>) -> Choice {
    move |k| match idle(k) {
        Choice::Wait(_) => order
            .iter()
            .copied()
            .find(|&s| ready(k, s))
            .map_or(Choice::Wait(Wait::NextEvent), cast),
        moving => moving,
    }
}

/// Charge in, then the core priority without Avatar or Bladestorm.
const CORE: &[SpellId] = &[
    CHARGE,
    COLOSSUS_SMASH,
    EXECUTE_MASSACRE,
    MORTAL_STRIKE,
    OVERPOWER,
    CLEAVE,
    SLAM,
];

struct Driver {
    k: Kernel<PartyMechanics>,
    trace: Vec<TraceRecord>,
}

impl Driver {
    fn new(f: &Fixture, seed: u64) -> Self {
        Self {
            k: f.kernel(seed),
            trace: Vec::new(),
        }
    }

    /// Plays `choose` until `stop` holds at a decision (`None`) or the run
    /// ends.
    fn until(
        &mut self,
        choose: impl Fn(&Kernel<PartyMechanics>) -> Choice,
        stop: impl Fn(&Kernel<PartyMechanics>) -> bool,
    ) -> Option<Outcome> {
        for _ in 0..200_000 {
            let step = self.k.advance().unwrap();
            self.trace.extend(self.k.drain_trace());
            match step {
                Step::Done(o) => return Some(o),
                Step::Decide(req) => {
                    assert_eq!(req.seat, SEAT);
                    if stop(&self.k) {
                        return None;
                    }
                    let choice = choose(&self.k);
                    self.k.submit(SEAT, choice).unwrap();
                }
            }
        }
        panic!("the run should end");
    }

    /// Submits `ability`, which must be ready, and plays on to the next
    /// decision; when it was cast.
    fn press(&mut self, ability: SpellId) -> SimTime {
        assert!(ready(&self.k, ability), "{ability:?}");
        let now = self.k.state().now();
        self.k.submit(SEAT, cast(ability)).unwrap();
        self.until(|_| Choice::Wait(Wait::NextEvent), |_| true);
        now
    }

    fn in_combat(&mut self) {
        let started = |k: &Kernel<PartyMechanics>| {
            k.state().enemies().first().is_some_and(|&e| {
                k.state().actor(e).is_some_and(|a| {
                    a.engaged
                        && k.state()
                            .distance(SEAT, e)
                            .is_some_and(|d| d <= MELEE_RANGE)
                })
            })
        };
        assert!(self.until(idle, started).is_none());
    }
}

fn ctx(k: &Kernel<PartyMechanics>, spell: SpellId, hit: HitKind) -> EffectCtx {
    EffectCtx {
        caster: me(k),
        target: Some(boss(k)),
        spell: Some(spell),
        aura: None,
        event_amount: None,
        scale: 1.0,
        spent: None,
        depth: 0,
        hand: None,
        hit,
        ground: None,
    }
}

/// Pre-crit damage per attack power for `spell` in the current state.
fn per_ap(m: &PartyMechanics, k: &Kernel<PartyMechanics>, spell: SpellId, hit: HitKind) -> f64 {
    m.math().outgoing(
        k.state(),
        &ctx(k, spell, hit),
        Coefficient::AttackPower(1.0),
        Outgoing::Damage(SchoolMask::PHYSICAL),
    )
}

fn crit(m: &PartyMechanics, k: &Kernel<PartyMechanics>, spell: SpellId) -> (f64, f64) {
    let c = ctx(k, spell, HitKind::Direct);
    (
        m.math().crit_chance(k.state(), &c),
        m.math().crit_multiplier(k.state(), &c),
    )
}

/// `(time, crit, amount)` for every hit by `spell`.
fn hits(trace: &[TraceRecord], spell: SpellId) -> Vec<(SimTime, bool, u64)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.spell == Some(spell) => Some((r.time, d.crit, d.amount)),
            _ => None,
        })
        .collect()
}

fn applied(trace: &[TraceRecord], aura: AuraId) -> Vec<SimTime> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::AuraApplied { aura: a, .. } if a == aura => Some(r.time),
            _ => None,
        })
        .collect()
}

/// Applications and refreshes of `aura`, not counting stacks taken away
/// (which the trace also reports as applied).
fn gains(trace: &[TraceRecord], aura: AuraId, before: u8) -> usize {
    let mut stacks = before;
    let mut n = 0;
    for r in trace {
        match r.event {
            TraceEvent::AuraApplied {
                aura: a, stacks: s, ..
            } if a == aura => {
                n += usize::from(s >= stacks);
                stacks = s;
            }
            TraceEvent::AuraRemoved { aura: a, .. } if a == aura => stacks = 0,
            _ => {}
        }
    }
    n
}

fn casts(trace: &[TraceRecord], spell: SpellId) -> usize {
    trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::CastEnd { spell: s, .. } if s == spell))
        .count()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9 * b.abs().max(1.0)
}

#[test]
fn the_build_has_its_abilities() {
    let f = fixture();
    let k = f.kernel(0);
    let abilities = k.state().abilities(SEAT);
    for s in [
        MORTAL_STRIKE,
        OVERPOWER,
        EXECUTE_MASSACRE,
        SLAM,
        COLOSSUS_SMASH,
        BLADESTORM,
        CLEAVE,
        CHARGE,
        WRECKING_THROW,
    ] {
        assert!(abilities.contains(&s), "{s:?}");
    }
    // Massacre's Execute replaces the plain one.
    assert!(!abilities.contains(&EXECUTE));
}

#[test]
fn talents_land_on_their_spells() {
    let f = fixture();
    let m = f.mechanics();
    let mut d = Driver::new(&f, 0);
    d.in_combat();
    let k = &d.k;
    // Charge has only what everything has; each spell's own multipliers
    // are its ratio to it.
    let base = per_ap(&m, k, CHARGE, HitKind::Direct);
    let ratio = |s, hit| per_ap(&m, k, s, hit) / base;
    let d_ = HitKind::Direct;
    // Sharpened Blades' two ranks.
    assert!(close(ratio(MORTAL_STRIKE, d_), 1.10));
    // Improved Overpower, and Strength of Arms' two ranks.
    assert!(close(ratio(OVERPOWER, d_), 1.15 * 1.10));
    // Barbaric Training and Strength of Arms.
    assert!(close(ratio(SLAM, d_), 1.10 * 1.10));
    assert!(close(ratio(HEROIC_STRIKE, d_), 1.10 * 1.10));
    // Barbaric Training and Sharpened Blades.
    assert!(close(ratio(CLEAVE, d_), 1.10 * 1.10));
    // The spec aura's Execute bonus and Sharpened Blades, on every Execute.
    for s in [EXECUTE_MASSACRE, EXECUTE_FREE] {
        assert!(close(ratio(s, d_), 1.17 * 1.10));
    }
    // Bloodborne's two ranks on the bleeds.
    assert!(close(ratio(DEEP_WOUNDS, HitKind::Periodic), 1.10));
    assert!(close(ratio(REND_BLEED, HitKind::Periodic), 1.10));

    let (charge_crit, charge_mult) = crit(&m, k, CHARGE);
    // Battle Stance and Stance Mastery's 3%, and Impale's 10%.
    assert!(close(charge_mult, 2.0 * 1.13));
    let extra = |s| {
        let (c, mult) = crit(&m, k, s);
        (c - charge_crit, mult / 2.0 - charge_mult / 2.0)
    };
    let pairs = [
        (MORTAL_STRIKE, (0.0, 0.10)),
        (OVERPOWER, (0.10, 0.10)),
        (SLAM, (0.10, 0.15)),
        (HEROIC_STRIKE, (0.10, 0.15)),
        (CLEAVE, (0.0, 0.15)),
        // Critical Thinking; Sharpened Blades and Cruel Strikes.
        (EXECUTE_MASSACRE, (0.10, 0.20)),
    ];
    for (s, (c, mult)) in pairs {
        let (gc, gm) = extra(s);
        assert!(close(gc, c) && close(gm, mult), "{s:?}: {gc} {gm}");
    }
    assert_eq!(crit(&m, k, WRECKING_THROW).0, 0.0);
    assert_eq!(crit(&m, k, FATALITY).0, 0.0);
}

#[test]
fn autos_give_rage_by_weapon_speed() {
    let mut f = fixture();
    f.dummy_health(100_000_000);
    for r in &mut f.template.resources {
        r.max = 1e9;
    }
    let mut d = Driver::new(&f, 3);
    d.in_combat();
    let start = rage(&d.k);
    let swings = |trace: &[TraceRecord]| -> Vec<bool> {
        trace
            .iter()
            .filter_map(|r| match r.event {
                TraceEvent::Damage(x) if x.spell.is_none() && x.source == ActorId(0) => {
                    Some(x.crit)
                }
                _ => None,
            })
            .collect()
    };
    let mark = d.trace.len();
    let tick =
        |k: &Kernel<PartyMechanics>| Choice::Wait(Wait::Until(k.state().now() + SimDuration(200)));
    let mut seen = Vec::new();
    while seen.iter().filter(|&&c| c).count() < 2 {
        let until = d.k.state().now() + SimDuration(1000);
        assert!(d.until(tick, |k| k.state().now() >= until).is_none());
        seen = swings(&d.trace[mark..]);
    }
    assert!(seen.contains(&false));
    // 6.125 rage per second of the equipped weapon's 3.6 s speed, War
    // Machine's 10% more, and Seasoned Soldier's 10% more on a crit (War
    // Machine's share included).
    let per = |crit: bool| 3.6 * (6.125 * 1.1 + if crit { 0.6125 * 1.1 } else { 0.0 });
    let want: f64 = seen.iter().map(|&c| per(c)).sum();
    let got = rage(&d.k) - start;
    assert!(close(got, want), "{got} {want} {seen:?}");
}

#[test]
fn execute_spends_up_to_forty_and_refunds_thirty_percent() {
    for (start, spent) in [(30.0, 30.0), (100.0, 40.0)] {
        let mut f = fixture();
        f.rage_only_from_casts(start);
        f.dummy_health(400_000);
        let m = f.mechanics();
        let mut d = Driver::new(&f, 4);
        d.in_combat();
        // Not on a healthy target.
        assert!(!ready(&d.k, EXECUTE_MASSACRE));
        let low = |k: &Kernel<PartyMechanics>| health_pct(k) <= 0.35;
        assert!(d.until(idle, low).is_none());
        assert_eq!(rage(&d.k), start);
        assert_eq!(buff(&d.k, SUDDEN_DEATH), 0);
        let scale = spent / 20.0;
        let want = {
            let k = &d.k;
            let c = EffectCtx {
                scale,
                ..ctx(k, EXECUTE_MASSACRE, HitKind::Direct)
            };
            m.math().outgoing(
                k.state(),
                &c,
                Coefficient::AttackPower(2.42125),
                Outgoing::Damage(SchoolMask::PHYSICAL),
            )
        };
        let mult = crit(&m, &d.k, EXECUTE_MASSACRE).1;
        let ms = per_ap(&m, &d.k, MORTAL_STRIKE, HitKind::Direct);
        let mark = d.trace.len();
        d.press(EXECUTE_MASSACRE);
        // Improved Execute's 10% back, and Critical Thinking's 10% a rank.
        assert!(
            close(rage(&d.k), start - spent + 0.3 * spent),
            "{start}: {}",
            rage(&d.k)
        );
        let hit = hits(&d.trace[mark..], EXECUTE_MASSACRE);
        assert_eq!(hit.len(), 1);
        let (_, crit, amount) = hit[0];
        let want = if crit { want * mult } else { want };
        assert_eq!(amount, whole_points(want), "{start}");
        // Executioner's Precision: Mortal Strike's next hit is 35% harder.
        assert_eq!(buff(&d.k, EXECUTIONERS_PRECISION), 1);
        let boosted = per_ap(&m, &d.k, MORTAL_STRIKE, HitKind::Direct);
        assert!(close(boosted / ms, 1.35));
    }
}

#[test]
fn sudden_death_makes_execute_free_anywhere() {
    let mut f = fixture();
    f.rage_only_from_casts(100.0);
    f.dummy_health(30_000_000);
    let mut d = Driver::new(&f, 5);
    d.in_combat();
    let proc = |k: &Kernel<PartyMechanics>| buff(k, SUDDEN_DEATH) > 0 && ready(k, EXECUTE_MASSACRE);
    assert!(d
        .until(first_of(&[MORTAL_STRIKE, OVERPOWER, SLAM]), proc)
        .is_none());
    assert!(health_pct(&d.k) > 0.35);
    assert_eq!(
        d.k.state().resolved_spell(SEAT, EXECUTE_MASSACRE),
        EXECUTE_FREE
    );
    let stacks = buff(&d.k, SUDDEN_DEATH);
    let before = rage(&d.k);
    let mark = d.trace.len();
    d.press(EXECUTE_MASSACRE);
    let window = &d.trace[mark..];
    assert_eq!(hits(window, EXECUTE_FREE).len(), 1);
    assert!(hits(window, EXECUTE_MASSACRE).is_empty());
    // Free: nothing spent.
    assert_eq!(rage(&d.k), before);
    // One stack goes as it lands; the cast may roll another after.
    let again = gains(window, SUDDEN_DEATH, stacks) as u8;
    assert_eq!(buff(&d.k, SUDDEN_DEATH), (stacks - 1 + again).min(2));
    assert_eq!(buff(&d.k, EXECUTIONERS_PRECISION), 1);
}

#[test]
fn master_of_warfare_turns_slam_into_heroic_strike() {
    let f = fixture();
    let m = f.mechanics();
    let mut d = Driver::new(&f, 6);
    d.in_combat();
    let proc = |k: &Kernel<PartyMechanics>| buff(k, WARFARE_PROC) > 0 && ready(k, SLAM);
    assert!(d
        .until(first_of(&[MORTAL_STRIKE, OVERPOWER, SLAM]), proc)
        .is_none());
    assert_eq!(d.k.state().resolved_spell(SEAT, SLAM), HEROIC_STRIKE);
    let ms = per_ap(&m, &d.k, MORTAL_STRIKE, HitKind::Direct);
    let ms_mult = crit(&m, &d.k, MORTAL_STRIKE).1;
    let stacks = buff(&d.k, WARFARE_STACK);
    let might = buff(&d.k, HEROIC_MIGHT_ACC);
    let mark = d.trace.len();
    d.press(SLAM);
    assert_eq!(hits(&d.trace[mark..], HEROIC_STRIKE).len(), 1);
    assert!(hits(&d.trace[mark..], SLAM).is_empty());
    assert_eq!(buff(&d.k, WARFARE_PROC), 0);
    assert_eq!(buff(&d.k, WARFARE_STACK), stacks + 1);
    assert_eq!(buff(&d.k, HEROIC_MIGHT_ACC), might + 1);
    // Each stack: 5% more Mortal Strike damage and critical damage.
    let n = f64::from(stacks);
    let after = per_ap(&m, &d.k, MORTAL_STRIKE, HitKind::Direct);
    assert!(close(
        after / ms,
        (1.0 + 0.05 * (n + 1.0)) / (1.0 + 0.05 * n)
    ));
    let after_mult = crit(&m, &d.k, MORTAL_STRIKE).1;
    assert!(close(after_mult - ms_mult, 2.0 * 0.05));
}

#[test]
fn heroic_might_powers_up_for_colossus_smash() {
    let f = fixture();
    let m = f.mechanics();
    let mut d = Driver::new(&f, 7);
    d.in_combat();
    let ready_with_might =
        |k: &Kernel<PartyMechanics>| buff(k, HEROIC_MIGHT_ACC) >= 2 && ready(k, COLOSSUS_SMASH);
    let order: &[SpellId] = &[MORTAL_STRIKE, OVERPOWER, SLAM];
    assert!(d.until(first_of(order), ready_with_might).is_none());
    let stacks = buff(&d.k, HEROIC_MIGHT_ACC);
    let charge = per_ap(&m, &d.k, CHARGE, HitKind::Direct);
    let hit = IncomingHit {
        attacker: Some(me(&d.k)),
        ..IncomingHit::direct(SchoolMask::PHYSICAL)
    };
    let taken = m.math().mitigate(d.k.state(), boss(&d.k), 1000.0, hit);
    d.press(COLOSSUS_SMASH);
    assert_eq!(debuff(&d.k, COLOSSUS_SMASH_DEBUFF), 1);
    assert_eq!(buff(&d.k, HEROIC_MIGHT_ACC), 0);
    assert_eq!(buff(&d.k, HEROIC_MIGHT), stacks);
    let ratio = per_ap(&m, &d.k, CHARGE, HitKind::Direct) / charge;
    assert!(
        close(ratio, 1.0 + f64::from(stacks) * 0.03 / 1.3),
        "{ratio}"
    );
    // The debuff: 30% more from this warrior.
    let now = m.math().mitigate(d.k.state(), boss(&d.k), 1000.0, hit);
    assert!(close(now / taken, 1.3));
}

#[test]
fn cleave_rends_and_bloodletting_sharpens_against_it() {
    let f = fixture();
    let m = f.mechanics();
    let mut d = Driver::new(&f, 8);
    d.in_combat();
    let enough = |k: &Kernel<PartyMechanics>| rage(k) >= 20.0 && ready(k, CLEAVE);
    assert!(d.until(first_of(&[CHARGE]), enough).is_none());
    let before = crit(&m, &d.k, MORTAL_STRIKE).0;
    let charge_before = crit(&m, &d.k, CHARGE).0;
    let t = d.press(CLEAVE);
    assert_eq!(debuff(&d.k, REND_DOT), 1);
    assert!(close(crit(&m, &d.k, MORTAL_STRIKE).0 - before, 0.05));
    assert!(close(crit(&m, &d.k, CHARGE).0 - charge_before, 0.05));
    // The bleed ticks every 3 s, hasted, for 15 s, and once more for
    // what's left.
    let wait = |k: &Kernel<PartyMechanics>| k.state().now() > t + SimDuration(16_000);
    assert!(d.until(idle, wait).is_none());
    let ticks: Vec<SimTime> = hits(&d.trace, REND_BLEED).iter().map(|h| h.0).collect();
    assert_eq!(ticks.len(), 6, "{ticks:?}");
    assert_eq!(ticks[5], t + SimDuration(15_000));
    assert!(ticks[0] > t + SimDuration(2500) && ticks[0] < t + SimDuration(3000));
}

#[test]
fn deep_wounds_follows_executes_and_crits() {
    let f = fixture();
    let mut total_applied = 0;
    let mut isolated = 0;
    for seed in 0..4 {
        let mut d = Driver::new(&f, seed);
        assert!(d.until(first_of(CORE), |_| false).is_some());
        let causes: Vec<SimTime> = [EXECUTE_MASSACRE, EXECUTE_FREE]
            .iter()
            .flat_map(|&s| hits(&d.trace, s))
            .map(|(t, _, _)| t)
            .chain(
                [MORTAL_STRIKE, SLAM, HEROIC_STRIKE]
                    .iter()
                    .flat_map(|&s| hits(&d.trace, s))
                    .filter(|&(_, crit, _)| crit)
                    .map(|(t, _, _)| t),
            )
            .collect();
        let dw = applied(&d.trace, DEEP_WOUNDS_DOT);
        for t in &dw {
            assert!(causes.contains(t), "{t:?}");
        }
        // All but an Execute that kills.
        assert!(causes.len() - dw.len() <= 1, "{dw:?} {causes:?}");
        total_applied += dw.len();
        // With Bloodletting each lasts 7.98 s, ticking each second and
        // once more for the last 0.98.
        let ticks: Vec<SimTime> = hits(&d.trace, DEEP_WOUNDS).iter().map(|h| h.0).collect();
        let end = d.k.state().now();
        for (i, &t) in dw.iter().enumerate() {
            let alone = dw.get(i + 1).is_none_or(|&n| n > t + SimDuration(8000));
            let fresh = i == 0 || dw[i - 1] + SimDuration(8000) < t;
            if alone && fresh && t + SimDuration(8000) < end {
                let mine: Vec<SimTime> = ticks
                    .iter()
                    .copied()
                    .filter(|&at| at > t && at <= t + SimDuration(8000))
                    .collect();
                assert_eq!(mine.len(), 8, "{mine:?}");
                assert_eq!(mine[7], t + SimDuration(7980));
                isolated += 1;
            }
        }
    }
    assert!(total_applied > 0);
    assert!(isolated > 0);
}

#[test]
fn fatality_marks_go_off_with_executes() {
    let f = fixture();
    let mut fired = 0;
    for seed in 0..6 {
        let mut d = Driver::new(&f, seed);
        assert!(d.until(first_of(CORE), |_| false).is_some());
        let executes: Vec<SimTime> = [EXECUTE_MASSACRE, EXECUTE_FREE]
            .iter()
            .flat_map(|&s| hits(&d.trace, s))
            .map(|(t, _, _)| t)
            .collect();
        let marks: Vec<SimTime> = hits(&d.trace, MORTAL_STRIKE)
            .iter()
            .map(|&(t, _, _)| t)
            .collect();
        for (t, crit, _) in hits(&d.trace, FATALITY) {
            assert!(!crit);
            assert!(executes.contains(&t));
            assert!(marks.iter().any(|&m| m < t));
            fired += 1;
        }
        // Each one clears the marks: another needs a fresh one first.
        let mut marked = false;
        for r in &d.trace {
            match r.event {
                TraceEvent::AuraApplied { aura, .. } if aura == FATAL_MARK => marked = true,
                TraceEvent::Damage(x) if x.spell == Some(FATALITY) => {
                    assert!(marked, "{:?}", r.time);
                    marked = false;
                }
                _ => {}
            }
        }
        assert!(applied(&d.trace, FATAL_MARK).len() <= marks.len());
    }
    assert!(fired > 0);
}

#[test]
fn anger_management_cuts_colossus_smash_by_rage_spent() {
    let mut f = fixture();
    f.rage_only_from_casts(100.0);
    let mut d = Driver::new(&f, 9);
    d.in_combat();
    let t = d.press(COLOSSUS_SMASH);
    let ends = |k: &Kernel<PartyMechanics>| {
        k.state()
            .cooldown(me(k), COLOSSUS_SMASH)
            .and_then(|c| c.next_charge_at)
            .unwrap()
    };
    assert_eq!(ends(&d.k), t + SimDuration(45_000));
    // Just Warming Up's 15 back.
    assert_eq!(rage(&d.k), 115.0);
    d.until(idle, |k| ready(k, SLAM));
    d.press(SLAM);
    d.until(idle, |k| ready(k, MORTAL_STRIKE));
    d.press(MORTAL_STRIKE);
    // 50 spent (Frothing Berserker may have given 3 back): 2.5 s.
    assert_eq!(ends(&d.k), t + SimDuration(42_500));
}

#[test]
fn tactician_resets_overpower() {
    let f = fixture();
    let mut overpowers = 0usize;
    let mut bound = 0usize;
    for seed in 0..4 {
        let mut d = Driver::new(&f, seed);
        assert!(d.until(first_of(CORE), |_| false).is_some());
        overpowers += casts(&d.trace, OVERPOWER);
        let start = d.trace.iter().find_map(|r| match r.event {
            TraceEvent::CombatStart { .. } => Some(r.time),
            _ => None,
        });
        let secs = (d.k.state().now() - start.unwrap()).millis() / 1000;
        // Two charges, one back every 12 s.
        bound += 2 + secs as usize / 12;
    }
    assert!(overpowers > bound, "{overpowers} {bound}");
}

#[test]
fn bladestorm_strikes_seven_times_and_autos_go_on() {
    let f = fixture();
    let mut d = Driver::new(&f, 10);
    d.in_combat();
    let t = d.press(BLADESTORM);
    let done = |k: &Kernel<PartyMechanics>| k.state().now() >= t + SimDuration(7000);
    assert!(d.until(idle, done).is_none());
    let strikes = hits(&d.trace, BLADESTORM_HIT);
    assert_eq!(strikes.len(), 7, "{strikes:?}");
    assert_eq!(strikes[0].0, t);
    let gaps: Vec<u32> = strikes
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).millis())
        .collect();
    // A second hasted, to the millisecond.
    assert!(
        gaps.iter().all(|&g| g.abs_diff(gaps[0]) <= 1 && g < 1000),
        "{gaps:?}"
    );
    let autos = d
        .trace
        .iter()
        .filter(|r| {
            r.time > t
                && r.time < strikes[6].0
                && matches!(r.event, TraceEvent::Damage(x) if x.spell.is_none() && x.source == ActorId(0))
        })
        .count();
    assert!(autos >= 1);
}

fn aura_instance(k: &Kernel<PartyMechanics>, holder: ActorId, aura: AuraId) -> AuraInstance {
    *k.state()
        .auras(holder)
        .iter()
        .find(|a| a.aura == aura)
        .unwrap()
}

fn bladestorm_ready_at(k: &Kernel<PartyMechanics>) -> SimTime {
    k.state()
        .cooldown(me(k), BLADESTORM)
        .and_then(|c| c.next_charge_at)
        .unwrap()
}

#[test]
fn slayers_strike_procs_off_the_listed_hits_with_executioner() {
    let f = slayer();
    let mut d = Driver::new(&f, 1);
    assert!(d.until(first_of(CORE), |_| false).is_some());
    let strikes = hits(&d.trace, SLAYERS_STRIKE);
    let sources: Vec<SimTime> = SLAYER_SOURCES
        .iter()
        .flat_map(|&s| hits(&d.trace, s))
        .map(|h| h.0)
        .collect();
    assert!(strikes.len() > 50, "{}", strikes.len());
    // Each on a hit from the list, at least 0.5 s apart.
    for s in &strikes {
        assert!(sources.contains(&s.0), "{s:?}");
    }
    assert!(strikes
        .windows(2)
        .all(|w| w[1].0 - w[0].0 >= SimDuration(500)));
    // Arms's 25%, with bad-luck protection, on a little under every hit
    // the cooldown lets through.
    let rate = strikes.len() as f64 / sources.len() as f64;
    assert!((0.2..0.3).contains(&rate), "{rate}");
    assert_eq!(gains(&d.trace, EXECUTIONER, 0), strikes.len());
}

#[test]
fn executioner_lasts_longer_with_deadly_focus_and_stacks_apart() {
    for (deadly_focus, lasts) in [(true, 18_000), (false, 12_000)] {
        let mut f = slayer();
        if !deadly_focus {
            f.without(DEADLY_FOCUS);
        }
        let mut d = Driver::new(&f, 2);
        d.in_combat();
        let two = |k: &Kernel<PartyMechanics>| buff(k, EXECUTIONER) >= 2;
        assert!(d.until(first_of(CORE), two).is_none());
        let strikes = hits(&d.trace, SLAYERS_STRIKE);
        let ex = aura_instance(&d.k, me(&d.k), EXECUTIONER);
        assert_eq!(ex.expires, Some(strikes[1].0 + SimDuration(lasts)));
        // The first stack keeps its own timer.
        assert_eq!(
            ex.next_stack_expires,
            Some(strikes[0].0 + SimDuration(lasts))
        );
    }
}

#[test]
fn slayer_talents_land_on_their_spells() {
    let core = fixture();
    let mc = core.mechanics();
    let mut dc = Driver::new(&core, 0);
    dc.in_combat();
    let f = slayer();
    let m = f.mechanics();
    let mut d = Driver::new(&f, 0);
    d.in_combat();
    // Each spell's multiplier over the spec tree's, Charge's ratio taken
    // out.
    let over = |s: SpellId| {
        let mine = per_ap(&m, &d.k, s, HitKind::Direct) / per_ap(&m, &d.k, CHARGE, HitKind::Direct);
        let theirs =
            per_ap(&mc, &dc.k, s, HitKind::Direct) / per_ap(&mc, &dc.k, CHARGE, HitKind::Direct);
        mine / theirs
    };
    // Slayer's Malice.
    for s in [OVERPOWER, EXECUTE_MASSACRE, EXECUTE_FREE] {
        assert!(close(over(s), 1.15), "{s:?}");
    }
    // Unrelenting Onslaught.
    assert!(close(over(BLADESTORM_HIT), 1.20));
    assert!(close(over(MORTAL_STRIKE), 1.0));
    // Unhinged's Mortal Strike is Mortal Strike's twin.
    assert_eq!(
        f.data.spells[&UNHINGED_MS].effects,
        f.data.spells[&MORTAL_STRIKE].effects
    );
    assert!(close(
        per_ap(&m, &d.k, UNHINGED_MS, HitKind::Direct),
        per_ap(&m, &d.k, MORTAL_STRIKE, HitKind::Direct)
    ));
    assert_eq!(crit(&m, &d.k, UNHINGED_MS), crit(&m, &d.k, MORTAL_STRIKE));
}

#[test]
fn executioner_sharpens_execute_and_show_no_mercy_its_crits() {
    for show_no_mercy in [true, false] {
        let mut f = slayer();
        if !show_no_mercy {
            f.without(SHOW_NO_MERCY);
        }
        let m = f.mechanics();
        let mut d = Driver::new(&f, 3);
        d.in_combat();
        let execute = |k: &Kernel<PartyMechanics>| {
            let (c, mult) = crit(&m, k, EXECUTE_MASSACRE);
            let (cc, cmult) = crit(&m, k, CHARGE);
            (
                per_ap(&m, k, EXECUTE_MASSACRE, HitKind::Direct)
                    / per_ap(&m, k, CHARGE, HitKind::Direct),
                c - cc,
                mult / 2.0 - cmult / 2.0,
            )
        };
        let (base, base_c, base_m) = execute(&d.k);
        let three = |k: &Kernel<PartyMechanics>| buff(k, EXECUTIONER) >= 3;
        let order: &[SpellId] = &[MORTAL_STRIKE, OVERPOWER, SLAM];
        assert!(d.until(first_of(order), three).is_none());
        let n = f64::from(buff(&d.k, EXECUTIONER));
        let (now, c, mult) = execute(&d.k);
        assert!(close(now / base, 1.0 + 0.03 * n));
        let more = if show_no_mercy { 0.03 * n } else { 0.0 };
        assert!(close(c - base_c, more), "{c} {base_c}");
        assert!(close(mult - base_m, more), "{mult} {base_m}");
    }
}

#[test]
fn imminent_demise_turns_three_slayers_strikes_into_sudden_death() {
    let mut f = slayer();
    // Only Imminent Demise's Sudden Death.
    f.data
        .auras
        .get_mut(&SUDDEN_DEATH_TALENT)
        .unwrap()
        .listeners
        .clear();
    let mut d = Driver::new(&f, 4);
    d.in_combat();
    let start = d.k.state().now();
    let order: &[SpellId] = &[MORTAL_STRIKE, OVERPOWER, SLAM];
    let later = |k: &Kernel<PartyMechanics>| k.state().now() >= start + SimDuration(90_000);
    assert!(d.until(first_of(order), later).is_none());
    let strikes = hits(&d.trace, SLAYERS_STRIKE).len();
    assert!(strikes >= 6, "{strikes}");
    assert_eq!(gains(&d.trace, SUDDEN_DEATH, 0), strikes / 3);
}

/// Sudden Death up, and Execute ready, on the Slayer build.
fn until_sudden_death(d: &mut Driver) {
    let proc = |k: &Kernel<PartyMechanics>| buff(k, SUDDEN_DEATH) > 0 && ready(k, EXECUTE_MASSACRE);
    let order: &[SpellId] = &[MORTAL_STRIKE, OVERPOWER, SLAM];
    assert!(d.until(first_of(order), proc).is_none());
}

#[test]
fn sudden_death_executes_reap_the_storm_and_cut_bladestorm() {
    let f = slayer();
    let mut seen = 0;
    for seed in 0..8 {
        let mut d = Driver::new(&f, seed);
        d.in_combat();
        d.press(BLADESTORM);
        let after = d.k.state().now() + SimDuration(7000);
        assert!(d.until(idle, |k| k.state().now() >= after).is_none());
        until_sudden_death(&mut d);
        let executioner = buff(&d.k, EXECUTIONER);
        let overwhelmed = debuff(&d.k, OVERWHELMED);
        let demise = buff(&d.k, IMMINENT_DEMISE);
        let ready_at = bladestorm_ready_at(&d.k);
        let mark = d.trace.len();
        d.press(EXECUTE_MASSACRE);
        let window = &d.trace[mark..];
        assert_eq!(hits(window, EXECUTE_FREE).len(), 1);
        assert_eq!(hits(window, REAP_THE_STORM).len(), 1);
        assert_eq!(buff(&d.k, IMMINENT_DEMISE), demise + 1);
        // A Slayer's Strike off the Execute would add a stack first.
        if !hits(window, SLAYERS_STRIKE).is_empty() || executioner == 0 {
            continue;
        }
        seen += 1;
        let n = u32::from(executioner);
        assert_eq!(
            ready_at - bladestorm_ready_at(&d.k),
            SimDuration(5000 * n),
            "{seed}"
        );
        // A stack per Executioner stack, and one from Reap the Storm.
        assert_eq!(
            debuff(&d.k, OVERWHELMED),
            (overwhelmed + executioner + 1).min(5)
        );
    }
    assert!(seen >= 2, "{seen}");
}

#[test]
fn only_sudden_death_executes_reap_the_storm() {
    let mut f = slayer();
    f.dummy_health(20_000_000);
    let mut d = Driver::new(&f, 5);
    assert!(d.until(first_of(CORE), |_| false).is_some());
    let free = hits(&d.trace, EXECUTE_FREE).len();
    assert!(free > 10 && !hits(&d.trace, EXECUTE_MASSACRE).is_empty());
    assert_eq!(hits(&d.trace, REAP_THE_STORM).len(), free);
}

#[test]
fn imminent_demise_fits_more_bladestorm_into_the_same_time() {
    let f = slayer();
    let m = f.mechanics();
    let mut d = Driver::new(&f, 6);
    d.in_combat();
    for _ in 0..2 {
        until_sudden_death(&mut d);
        d.press(EXECUTE_MASSACRE);
    }
    assert!(d.until(idle, |k| ready(k, BLADESTORM)).is_none());
    let stacks = buff(&d.k, IMMINENT_DEMISE);
    assert!(stacks >= 2, "{stacks}");
    let haste = m.math().haste_mult(d.k.state(), me(&d.k));
    let mark = d.trace.len();
    let t = d.press(BLADESTORM);
    let done = |k: &Kernel<PartyMechanics>| k.state().now() >= t + SimDuration(7000);
    assert!(d.until(idle, done).is_none());
    let window = &d.trace[mark..];
    let strikes = hits(window, BLADESTORM_HIT);
    let ticks = 7 + usize::from(stacks);
    assert_eq!(strikes.len(), ticks);
    let span = f64::from((strikes[ticks - 1].0 - t).millis());
    assert!((span - 6000.0 / haste).abs() <= 2.0, "{span} {haste}");
    assert_eq!(buff(&d.k, IMMINENT_DEMISE), 0);
    // Unhinged's Mortal Strike on every other tick after the first.
    let unhinged: Vec<SimTime> = hits(window, UNHINGED_MS).iter().map(|h| h.0).collect();
    let odd: Vec<SimTime> = strikes.iter().skip(1).step_by(2).map(|h| h.0).collect();
    assert_eq!(unhinged, odd);
}

#[test]
fn bladestorm_overwhelms_and_leaves_violent_euphoria() {
    let f = slayer();
    let m = f.mechanics();
    let mut d = Driver::new(&f, 7);
    d.in_combat();
    let haste = m.math().haste_mult(d.k.state(), me(&d.k));
    let taken = |k: &Kernel<PartyMechanics>, spell: Option<SpellId>| {
        let hit = IncomingHit {
            attacker: Some(me(k)),
            spell,
            ..IncomingHit::direct(SchoolMask::PHYSICAL)
        };
        m.math().mitigate(k.state(), boss(k), 1000.0, hit)
    };
    let before = taken(&d.k, Some(MORTAL_STRIKE)) / taken(&d.k, None);
    let mark = d.trace.len();
    let t = d.press(BLADESTORM);
    let done = |k: &Kernel<PartyMechanics>| buff(k, VIOLENT_EUPHORIA) > 0;
    assert!(d.until(idle, done).is_none());
    let window = &d.trace[mark..];
    assert_eq!(hits(window, UNHINGED_MS).len(), 3);
    assert_eq!(debuff(&d.k, OVERWHELMED), 5);
    let after = taken(&d.k, Some(MORTAL_STRIKE)) / taken(&d.k, None);
    assert!(close(after / before, 1.05));
    assert!(close(
        m.math().haste_mult(d.k.state(), me(&d.k)) / haste,
        1.15
    ));
    let euphoria = aura_instance(&d.k, me(&d.k), VIOLENT_EUPHORIA);
    let last = hits(&d.trace, BLADESTORM_HIT).last().unwrap().0;
    assert!(last - t < SimDuration(6000));
    assert_eq!(euphoria.expires, Some(last + SimDuration(8000)));
}

#[test]
fn culling_cyclone_sharpens_bladestorm_against_one_target() {
    let strikes = |cyclone: bool| {
        let mut f = slayer();
        if !cyclone {
            f.without(CULLING_CYCLONE);
        }
        let mut d = Driver::new(&f, 8);
        d.in_combat();
        let t = d.press(BLADESTORM);
        let done = |k: &Kernel<PartyMechanics>| k.state().now() >= t + SimDuration(7000);
        assert!(d.until(idle, done).is_none());
        hits(&d.trace, BLADESTORM_HIT)
    };
    let (with, without) = (strikes(true), strikes(false));
    assert_eq!(with.len(), without.len());
    for (a, b) in with.iter().zip(&without) {
        assert_eq!((a.0, a.1), (b.0, b.1));
        assert!((a.2 as f64 / b.2 as f64 - 1.2).abs() < 1e-4, "{a:?} {b:?}");
    }
}

#[test]
fn fierce_followthrough_follows_a_mortal_strike_crit() {
    let order: &[SpellId] = &[MORTAL_STRIKE, OVERPOWER, SLAM];
    let up =
        |k: &Kernel<PartyMechanics>| buff(k, FIERCE_FOLLOWTHROUGH) > 0 && ready(k, MORTAL_STRIKE);
    let mut misses = 0;
    for seed in 0..6 {
        let f = slayer();
        let m = f.mechanics();
        let mut d = Driver::new(&f, seed);
        d.in_combat();
        assert!(d.until(first_of(order), up).is_none());
        // It came with a Mortal Strike crit.
        let granted = applied(&d.trace, FIERCE_FOLLOWTHROUGH);
        let crits: Vec<SimTime> = hits(&d.trace, MORTAL_STRIKE)
            .iter()
            .filter(|h| h.1)
            .map(|h| h.0)
            .collect();
        assert_eq!(granted, crits[..granted.len()].to_vec());
        // 20% on the next, and gone with it unless that crits too.
        let mut plain = slayer();
        plain
            .data
            .auras
            .get_mut(&FIERCE_FOLLOWTHROUGH)
            .unwrap()
            .modifiers
            .clear();
        let pm = plain.mechanics();
        let mut p = Driver::new(&plain, seed);
        p.in_combat();
        assert!(p.until(first_of(order), up).is_none());
        let ms = |m: &PartyMechanics, k: &Kernel<PartyMechanics>| {
            per_ap(m, k, MORTAL_STRIKE, HitKind::Direct)
        };
        assert!(close(ms(&m, &d.k) / ms(&pm, &p.k), 1.2));
        let unhinged = |m: &PartyMechanics, k: &Kernel<PartyMechanics>| {
            per_ap(m, k, UNHINGED_MS, HitKind::Direct)
        };
        assert!(close(unhinged(&m, &d.k) / unhinged(&pm, &p.k), 1.2));
        let mark = d.trace.len();
        d.press(MORTAL_STRIKE);
        let crit = hits(&d.trace[mark..], MORTAL_STRIKE)[0].1;
        assert_eq!(buff(&d.k, FIERCE_FOLLOWTHROUGH), u8::from(crit));
        misses += usize::from(!crit);
    }
    assert!(misses >= 2, "{misses}");
}

#[test]
fn unhinged_counts_afresh_each_bladestorm() {
    const ORDER: &[SpellId] = &[
        CHARGE,
        BLADESTORM,
        COLOSSUS_SMASH,
        EXECUTE_MASSACRE,
        MORTAL_STRIKE,
        OVERPOWER,
        SLAM,
    ];
    let f = slayer();
    let mut d = Driver::new(&f, 9);
    let later = |k: &Kernel<PartyMechanics>| k.state().now() >= SimTime(300_000);
    assert!(d.until(first_of(ORDER), later).is_none());
    let strikes = hits(&d.trace, BLADESTORM_HIT);
    let mut storms: Vec<Vec<SimTime>> = Vec::new();
    for s in strikes {
        match storms.last_mut() {
            Some(storm) if s.0 - *storm.last().unwrap() < SimDuration(2000) => storm.push(s.0),
            _ => storms.push(vec![s.0]),
        }
    }
    assert!(storms.len() >= 4, "{}", storms.len());
    let unhinged: Vec<SimTime> = hits(&d.trace, UNHINGED_MS).iter().map(|h| h.0).collect();
    let odd: Vec<SimTime> = storms
        .iter()
        .flat_map(|storm| storm.iter().skip(1).step_by(2).copied())
        .collect();
    assert_eq!(unhinged, odd);
}

#[test]
fn cleave_rolls_slayers_strike_on_each_target_half_a_second_apart() {
    let mut f = slayer();
    f.spec.pulls[0].waves[0].mobs[0].1 = 3;
    // Every roll lands, so only the cooldown holds them back.
    for l in &mut f.data.auras.get_mut(&SLAYERS_DOMINANCE).unwrap().listeners {
        l.chance = ProcChance::Flat(1.0);
    }
    let mut d = Driver::new(&f, 10);
    d.in_combat();
    let start = d.k.state().now();
    let later = |k: &Kernel<PartyMechanics>| k.state().now() >= start + SimDuration(120_000);
    assert!(d.until(first_of(&[CLEAVE]), later).is_none());
    let cleaves = hits(&d.trace, CLEAVE);
    let strikes = hits(&d.trace, SLAYERS_STRIKE);
    // Three hits a Cleave, one Slayer's Strike.
    assert!(cleaves.len() > 30);
    assert_eq!(strikes.len() * 3, cleaves.len());
    assert!(strikes
        .windows(2)
        .all(|w| w[1].0 - w[0].0 >= SimDuration(500)));
}
