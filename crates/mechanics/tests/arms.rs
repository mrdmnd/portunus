//! The checked-in Arms data (SimC's reference build, spec tree only)
//! through the real mechanics and kernel.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::mechanics::{whole_points, HitKind};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, MoveGoal, Outcome, Readiness, RunSetup,
    SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::effect::Coefficient;
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

/// The spec aura and the two talents that give autos rage.
const AUTO_RAGE: [AuraId; 3] = [AuraId(137049), AuraId(279423), AuraId(262231)];

struct Fixture {
    data: GameData,
    enemies: EnemyData,
    template: ActorTemplate,
    spec: ScenarioSpec,
}

/// The reference build on a dummy of exactly the listed health.
fn fixture() -> Fixture {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    let enemies: EnemyData = EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    let loadout: Loadout = read_ron(&dir.join("loadouts/arms.ron")).unwrap();
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
    // 3.5 x 1.75 a second of weapon speed, War Machine's 10% more, and
    // Seasoned Soldier's 10% more on a crit (War Machine's share included).
    let per = |crit: bool| 3.6 * (6.125 * 1.1 + if crit { 0.6125 * 1.1 } else { 0.0 });
    let want: f64 = seen.iter().map(|&c| per(c)).sum();
    let got = rage(&d.k) - start;
    assert!(close(got, want), "{got} {want} {seen:?}");
}

#[test]
fn execute_spends_up_to_forty_and_refunds_a_tenth_a_point() {
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
    let f = fixture();
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
    assert!(rage(&d.k) >= before);
    // One stack goes as it lands; the cast may roll another after.
    let again = gains(window, SUDDEN_DEATH, stacks) as u8;
    assert_eq!(buff(&d.k, SUDDEN_DEATH), (stacks - 1 + again).min(2));
    assert_eq!(buff(&d.k, EXECUTIONERS_PRECISION), 1);
}

#[test]
fn sudden_death_procs_about_as_often_as_simc_says() {
    let mut f = fixture();
    f.dummy_health(30_000_000);
    let mut total = 0usize;
    let mut minutes = 0.0;
    for seed in 0..3 {
        let mut d = Driver::new(&f, seed);
        let o = d.until(first_of(CORE), |_| false);
        assert!(o.is_some());
        let procs = gains(&d.trace, SUDDEN_DEATH, 0);
        // Tactical Edge's one per Colossus Smash isn't the proc.
        total += procs - casts(&d.trace, COLOSSUS_SMASH);
        let combat = d.trace.iter().find_map(|r| match r.event {
            TraceEvent::CombatEnd { .. } => Some(r.time),
            _ => None,
        });
        let start = d.trace.iter().find_map(|r| match r.event {
            TraceEvent::CombatStart { .. } => Some(r.time),
            _ => None,
        });
        minutes += f64::from((combat.unwrap() - start.unwrap()).millis()) / 60_000.0;
    }
    let rate = total as f64 / minutes;
    // 1.7 a minute, hasted by the gear and Wild Strikes, a little more for
    // the bad-luck protection. Each run is ten minutes (the pull's timeout).
    assert!((1.6..2.3).contains(&rate), "{rate}");
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
