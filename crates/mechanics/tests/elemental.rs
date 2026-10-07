//! The checked-in Elemental data through the real mechanics and kernel.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{AuraId, Dist, HookKey, Seat, Seed, SimDuration, SpecId, SpellId};
use portunus_engine::mechanics::whole_points;
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, Outcome, Readiness, RunSetup, SeatSetup,
    StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::effect::Effect;
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{CombatMath, EffectCtx, KitIssue, Kits, PartyMechanics, SpecRegistry};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

const LIGHTNING_BOLT: SpellId = SpellId(188196);
const LIGHTNING_BOLT_OVERLOAD: SpellId = SpellId(45284);
const LAVA_BURST: SpellId = SpellId(51505);
const LAVA_BURST_OVERLOAD: SpellId = SpellId(77451);
const FLAME_SHOCK: SpellId = SpellId(188389);
const EARTH_SHOCK: SpellId = SpellId(8042);
const FLAME_SHOCK_DOT: AuraId = AuraId(188389);
const LAVA_SURGE_BUFF: AuraId = AuraId(77762);

/// The loadout's 2600 intellect.
const SPELL_POWER: f64 = 2600.0;
/// 5% base plus 250 crit rating at 700 per percent.
const CRIT_PCT: f64 = 5.0 + 250.0 / 700.0;
/// 8% base plus 250 mastery rating at 700 per percent.
const MASTERY_PCT: f64 = 8.0 + 250.0 / 700.0;
/// Doubled, then Elemental Fury's 15%.
const CRIT_MULT: f64 = 2.0 * 1.15;

struct Fixture {
    data: Arc<GameData>,
    enemies: Arc<EnemyData>,
    template: ActorTemplate,
    sampler: Sampler,
}

fn fixture() -> Fixture {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    let enemies: EnemyData = EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    let loadout: Loadout = read_ron(&dir.join("loadouts/elemental.ron")).unwrap();
    let template = Compiler.compile(&data, &loadout).unwrap();
    let spec: ScenarioSpec = read_ron(&dir.join("scenarios/target_dummy.ron")).unwrap();
    let enemies = Arc::new(enemies);
    let sampler = Sampler::new(spec, Arc::clone(&enemies)).unwrap();
    Fixture {
        data: Arc::new(data),
        enemies,
        template,
        sampler,
    }
}

fn setup(f: &Fixture, seed: u64) -> RunSetup {
    RunSetup {
        run: Arc::new(f.sampler.sample(Seed(seed))),
        data: Arc::clone(&f.data),
        enemies: Arc::clone(&f.enemies),
        seats: vec![SeatSetup {
            template: f.template.clone(),
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

fn kernel(f: &Fixture, seed: u64) -> Kernel<PartyMechanics> {
    let setup = setup(f, seed);
    let mechanics = PartyMechanics::new(&setup, Arc::new(Kits::standard())).unwrap();
    Kernel::new(setup, mechanics).unwrap()
}

fn cast(ability: SpellId) -> Choice {
    Choice::Cast {
        ability,
        target: TargetSel::Primary,
        opts: CastOpts::default(),
    }
}

/// Flame Shock if missing, then Lava Burst, Earth Shock, Lightning Bolt.
fn priority(k: &Kernel<PartyMechanics>, seat: Seat) -> Choice {
    let mask = k.legal(seat);
    let ready = |s: SpellId| mask.abilities.get(&s) == Some(&Readiness::Now);
    let state = k.state();
    let me = state.seats()[usize::from(seat.0)];
    let dot_up = state
        .target(me)
        .is_some_and(|t| state.auras(t).iter().any(|a| a.aura == FLAME_SHOCK_DOT));
    if ready(FLAME_SHOCK) && !dot_up {
        cast(FLAME_SHOCK)
    } else if ready(LAVA_BURST) {
        cast(LAVA_BURST)
    } else if ready(EARTH_SHOCK) {
        cast(EARTH_SHOCK)
    } else if ready(LIGHTNING_BOLT) {
        cast(LIGHTNING_BOLT)
    } else {
        Choice::Wait(Wait::NextEvent)
    }
}

/// Play with `priority` until the run ends or `stop` holds before a
/// decision; `Some` if the run ended.
fn play_until(
    k: &mut Kernel<PartyMechanics>,
    stop: impl Fn(&Kernel<PartyMechanics>) -> bool,
) -> Option<Outcome> {
    for _ in 0..100_000 {
        match k.advance().unwrap() {
            Step::Done(o) => return Some(o),
            Step::Decide(req) => {
                if stop(k) {
                    return None;
                }
                let choice = priority(k, req.seat);
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    panic!("the run should end");
}

fn rollout(f: &Fixture, seed: u64) -> (Outcome, Vec<TraceRecord>) {
    let mut k = kernel(f, seed);
    let outcome = play_until(&mut k, |_| false).unwrap();
    (outcome, k.drain_trace())
}

/// `(crit, amount)` for every hit by `spell`.
fn hits(trace: &[TraceRecord], spell: SpellId) -> Vec<(bool, u64)> {
    trace
        .iter()
        .filter_map(|r| match &r.event {
            TraceEvent::Damage(d) if d.spell == Some(spell) => Some((d.crit, d.amount)),
            _ => None,
        })
        .collect()
}

fn completed_casts(trace: &[TraceRecord]) -> BTreeMap<SpellId, usize> {
    let mut out = BTreeMap::new();
    for r in trace {
        if let TraceEvent::CastEnd { spell, .. } = r.event {
            *out.entry(spell).or_default() += 1;
        }
    }
    out
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn kits_cover_the_data() {
    let f = fixture();
    assert_eq!(Kits::standard().validate(&f.data), Vec::new());

    let mut data = (*f.data).clone();
    let mut spec = data.specs[&SpecId(262)].clone();
    spec.id = SpecId(1);
    data.specs.insert(SpecId(1), spec);
    data.spells
        .get_mut(&EARTH_SHOCK)
        .unwrap()
        .effects
        .push(Effect::Hook(HookKey("missing".into())));
    let issues = Kits::standard().validate(&data);
    assert!(issues.contains(&KitIssue::MissingKit(SpecId(1))));
    assert!(issues.contains(&KitIssue::UnknownHook(HookKey("missing".into()))));
}

#[test]
fn crit_chance_follows_flame_shock() {
    let f = fixture();
    let mut k = kernel(&f, 1);
    let dot_up = |k: &Kernel<PartyMechanics>| {
        let s = k.state();
        s.target(s.seats()[0])
            .is_some_and(|t| s.auras(t).iter().any(|a| a.aura == FLAME_SHOCK_DOT))
    };
    assert!(play_until(&mut k, dot_up).is_none());

    let setup = setup(&f, 1);
    let mechanics = PartyMechanics::new(&setup, Arc::new(Kits::standard())).unwrap();
    let state = k.state();
    let me = state.seats()[0];
    let ctx = |spell| EffectCtx {
        caster: me,
        target: state.target(me),
        spell: Some(spell),
        aura: None,
        event_amount: None,
        scale: 1.0,
        depth: 0,
    };
    let math = mechanics.math();
    assert!(close(
        math.crit_chance(state, &ctx(LIGHTNING_BOLT)),
        CRIT_PCT / 100.0
    ));
    assert_eq!(math.crit_chance(state, &ctx(LAVA_BURST)), 1.0);
    assert_eq!(math.crit_chance(state, &ctx(LAVA_BURST_OVERLOAD)), 1.0);
    let untargeted = EffectCtx {
        target: None,
        ..ctx(LAVA_BURST)
    };
    assert!(close(
        math.crit_chance(state, &untargeted),
        CRIT_PCT / 100.0
    ));
    assert!(close(
        math.crit_multiplier(state, &ctx(LAVA_BURST)),
        CRIT_MULT
    ));
}

#[test]
fn hits_follow_the_formulas() {
    let f = fixture();
    let (_, trace) = rollout(&f, 2);
    let bolts = hits(&trace, LIGHTNING_BOLT);
    assert!(!bolts.is_empty());
    let normal = 1.15 * SPELL_POWER;
    for &(crit, amount) in &bolts {
        let want = if crit { normal * CRIT_MULT } else { normal };
        assert_eq!(amount, whole_points(want), "{want}");
    }
    assert!(bolts.iter().any(|h| h.0), "some bolts should crit");

    // Lava Burst grows with crit chance, its Flame Shock crit aside.
    let lava = hits(&trace, LAVA_BURST);
    assert!(!lava.is_empty());
    let normal = 1.08 * SPELL_POWER * (1.0 + CRIT_PCT / 100.0);
    for &(crit, amount) in &lava {
        let want = if crit { normal * CRIT_MULT } else { normal };
        assert_eq!(amount, whole_points(want), "{want}");
    }
}

#[test]
fn rollout_clears_the_dummy() {
    let f = fixture();
    let (outcome, trace) = rollout(&f, 3);
    assert!(outcome.completed);
    let health = f
        .sampler
        .sample(Seed(3))
        .segments
        .iter()
        .find_map(|s| match s {
            portunus_scenario::resolved::Segment::Combat(c) => Some(c.spawns[0].max_health),
            portunus_scenario::resolved::Segment::Travel { .. } => None,
        });
    assert_eq!(
        outcome.seats[0].damage_done,
        health.unwrap(),
        "overkill excluded"
    );
    assert!(!hits(&trace, LIGHTNING_BOLT_OVERLOAD).is_empty());
    assert!(!hits(&trace, LAVA_BURST_OVERLOAD).is_empty());
    assert!(!hits(&trace, FLAME_SHOCK).is_empty());
}

#[test]
fn proc_rates_match_their_chances() {
    let f = fixture();
    let (mut bolts, mut bolt_overloads) = (0, 0);
    let (mut lava, mut lava_crits) = (0, 0);
    let (mut bolt_hits, mut bolt_crits) = (0, 0);
    let mut surges = 0;
    for seed in 0..30 {
        let (_, trace) = rollout(&f, seed);
        let casts = completed_casts(&trace);
        bolts += casts.get(&LIGHTNING_BOLT).copied().unwrap_or(0);
        bolt_overloads += casts.get(&LIGHTNING_BOLT_OVERLOAD).copied().unwrap_or(0);
        let l = hits(&trace, LAVA_BURST);
        lava += l.len();
        lava_crits += l.iter().filter(|h| h.0).count();
        let b = hits(&trace, LIGHTNING_BOLT);
        bolt_hits += b.len();
        bolt_crits += b.iter().filter(|h| h.0).count();
        surges += trace
            .iter()
            .filter(|r| {
                matches!(r.event, TraceEvent::AuraApplied { aura, .. } if aura == LAVA_SURGE_BUFF)
            })
            .count();
    }
    let rate = |n: usize, d: usize| n as f64 / d as f64;
    let overload = rate(bolt_overloads, bolts);
    assert!(
        (overload - MASTERY_PCT / 100.0).abs() < 0.025,
        "overload rate {overload} from {bolts} bolts"
    );
    assert!(rate(lava_crits, lava) > 0.9, "{lava_crits} of {lava}");
    assert!(
        rate(bolt_crits, bolt_hits) < 0.2,
        "{bolt_crits} of {bolt_hits}"
    );
    assert!(surges > 30, "{surges} surges");
}

#[test]
fn same_seed_same_rollout() {
    let f = fixture();
    let hash = |seed| {
        let mut k = kernel(&f, seed);
        play_until(&mut k, |_| false).unwrap();
        k.trace_hash()
    };
    assert_eq!(hash(5), hash(5));
    assert_ne!(hash(5), hash(6));
}
