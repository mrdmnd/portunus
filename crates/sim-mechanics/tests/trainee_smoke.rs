//! G0 smoke tests: the whole stack — toy codex (pure effect IR) through the
//! kernel — kills targets deterministically; chained engagement works; and a
//! compiled run definition drives a full multi-pull rollout.

use std::path::Path;

use sim_encounter::{bestiary, RunSampler, RunSpec, ScenarioSampler};
use sim_engine::{Engine, ReferenceEngine, RolloutContext, Step};
use sim_mechanics::{toy, IrMechanics};
use sim_types::{
    from_millis, Action, CompiledActor, EngageSpec, Outcome, Plan, ResolvedCombat, ResolvedRun,
    ResolvedSpawn, Seed, SlotId, TargetSel, TriggerExpr, WaitSpec, WindowAnchor,
};

fn fixture(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(rel)
}

fn trainee() -> (CompiledActor, IrMechanics) {
    let (actor, arena) =
        toy::compile_path(&fixture("toy_codex/trainee.toml")).expect("toy codex compiles");
    (actor, IrMechanics::new(arena))
}

fn spawn(label: &str, hp: f64, engage: EngageSpec) -> ResolvedSpawn {
    ResolvedSpawn {
        label: label.into(),
        enemy: "dummy".into(),
        max_health: hp,
        count: 0,
        engage,
        script: vec![],
    }
}

fn dummy_run(hp: f64, timeout_ms: u64) -> ResolvedRun {
    ResolvedRun::single_combat(ResolvedCombat {
        name: "dummy".into(),
        spawns: vec![spawn("dummy#1", hp, EngageSpec::AtStart)],
        timeout: from_millis(timeout_ms),
    })
}

/// Fixed-priority scripted policy over the trainee kit:
/// surge > heavy_blow > rend > strike, else wait. (Slot order from
/// trainee.toml: 0 strike, 1 heavy_blow, 2 rend, 3 surge, 4 focus_strike.)
const PRIORITY: [u16; 4] = [3, 1, 2, 0];

fn step_policy(engine: &mut ReferenceEngine<'_, IrMechanics>) -> Option<Outcome> {
    match engine.next_decision() {
        Step::Decide(dp) => {
            let mask = engine.legal(dp.actor);
            let action = PRIORITY
                .into_iter()
                .map(SlotId)
                .find(|s| mask.is_castable(*s))
                .map(|slot| Action::Cast { slot, target: TargetSel::Primary })
                .unwrap_or(Action::Wait(WaitSpec::NextEvent));
            engine.submit(dp.actor, action).expect("scripted policy submits legally");
            None
        }
        Step::Done(o) => Some(o),
    }
}

fn drive_to_outcome(engine: &mut ReferenceEngine<'_, IrMechanics>) -> (Outcome, u64) {
    loop {
        if let Some(o) = step_policy(engine) {
            return (o, engine.trace_hash());
        }
    }
}

#[test]
fn toy_codex_compiles_the_full_kit() {
    let (actor, _) = trainee();
    assert_eq!(actor.name, "Trainee");
    assert_eq!(actor.abilities.len(), 5, "five abilities");
    assert_eq!(actor.auras.len(), 2, "frenzy + bleed");
    assert_eq!(actor.resources.len(), 1);
    // The proc is nested effect IR, not code: strike's range contains it.
    assert!(actor.abilities.iter().any(|a| a.name == "strike" && a.effect.len == 2));
}

#[test]
fn trainee_kills_the_dummy_deterministically() {
    let (actor, mechanics) = trainee();
    let roster = [actor];
    let run = dummy_run(30_000.0, 300_000);
    let plan = Plan::default();

    let go = |seed: u64| {
        let mut engine = ReferenceEngine::new(mechanics.clone());
        engine
            .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(seed) })
            .unwrap();
        drive_to_outcome(&mut engine)
    };

    let (o1, h1) = go(42);
    let (o2, h2) = go(42);

    let kill = o1.kill_time.expect("the dummy must die");
    assert!(kill > 0 && kill < from_millis(300_000));
    assert_eq!(o1, o2, "same seed => bit-identical outcome (invariant 1)");
    assert_eq!(h1, h2, "same seed => identical event trace");
    assert_eq!(o1.deaths, 0);
    assert!(o1.per_actor[0].damage_done >= 30_000.0);

    // Different seed => different proc rolls; the trace may differ (and the
    // engine must still terminate correctly).
    let (o3, _) = go(43);
    assert!(o3.kill_time.is_some());
}

/// Chained engagement: the second spawn enters only once the first wave is
/// below the HP threshold — observed as the targetable count changing
/// strictly after combat start.
#[test]
fn chain_condition_engages_mid_fight() {
    let (actor, mechanics) = trainee();
    let roster = [actor];
    let run = ResolvedRun::single_combat(ResolvedCombat {
        name: "chain".into(),
        spawns: vec![
            spawn("a#1", 20_000.0, EngageSpec::AtStart),
            spawn(
                "b#1",
                10_000.0,
                EngageSpec::When(TriggerExpr::EngagedHpFracBelow(0.5)),
            ),
        ],
        timeout: from_millis(300_000),
    });
    let plan = Plan::default();

    let mut engine = ReferenceEngine::new(mechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(5) })
        .unwrap();

    let mut saw_one_target = false;
    let mut chain_time = None;
    let outcome = loop {
        let targetable = engine.state().targetable_enemies().count();
        if targetable == 1 {
            saw_one_target = true;
        }
        if targetable == 2 && chain_time.is_none() {
            chain_time = Some(engine.now());
            // The chain fired at the declared threshold: engaged HP (both
            // spawns now) must be below 50% of the first wave's max… i.e.
            // the first spawn was at <10k when b#1 joined.
            let a = &engine.state().actors[1];
            assert!(a.health.value_at(engine.now()) < 10_000.0 + 1e-6);
        }
        if let Some(o) = step_policy(&mut engine) {
            break o;
        }
    };

    assert!(saw_one_target, "combat opened with only the first wave");
    let t = chain_time.expect("chain condition must fire");
    assert!(t > 0, "chain cannot fire at combat start");
    assert!(outcome.kill_time.is_some(), "both waves die");
}

/// Snapshot mid-fight (with auras, ticks, and cooldowns in flight), finish,
/// restore, finish again: identical outcomes (invariant 7).
#[test]
fn snapshot_restore_mid_fight_is_exact() {
    let (actor, mechanics) = trainee();
    let roster = [actor];
    let run = dummy_run(30_000.0, 300_000);
    let plan = Plan::default();

    let mut engine = ReferenceEngine::new(mechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(9) })
        .unwrap();

    // Advance 25 decisions: bleeds ticking, cooldowns rolling.
    for _ in 0..25 {
        assert!(step_policy(&mut engine).is_none(), "dummy died too fast for the test setup");
    }

    let snap = engine.snapshot();
    let (first, h1) = drive_to_outcome(&mut engine);
    engine.restore(&snap);
    let (second, h2) = drive_to_outcome(&mut engine);

    assert_eq!(first, second);
    assert_eq!(h1, h2);
}

/// The full slice: run definition + bestiary -> sampled ResolvedRun ->
/// multi-pull rollout with travel, timed body-pulls, a chain condition, and
/// trash on the boss — deterministically.
#[test]
fn compiled_run_executes_end_to_end() {
    let spec =
        RunSpec::from_path(&fixture("runs/toy_hall.run.toml")).expect("run file parses");
    let beasts =
        bestiary::load_path(&fixture("toy_codex/bestiary.toml")).expect("bestiary parses");
    let sampler = RunSampler::new(spec, beasts).expect("run validates");
    let run = sampler.sample(Seed(1234));
    let plan = sampler.plan();

    // Static cooldown assignments became plan windows policies can read.
    let opener = plan.pull("opener").expect("opener planned");
    assert!(opener.lust);
    assert!(opener
        .windows
        .iter()
        .any(|w| matches!(&w.anchor, WindowAnchor::ScriptEvent { spawn, script, occurrence: 2 }
            if spawn == "grunt#1" && script == "swipe")));

    let (actor, mechanics) = trainee();
    let roster = [actor];

    let go = || {
        let mut engine = ReferenceEngine::new(mechanics.clone());
        engine
            .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(1234) })
            .unwrap();
        drive_to_outcome(&mut engine)
    };

    let (o1, h1) = go();
    let (o2, h2) = go();
    assert_eq!(o1, o2, "same seed => identical outcome");
    assert_eq!(h1, h2, "same seed => identical event trace");

    // The run has leading travel on the first pull; any completion (kill or
    // wipe) must come after it.
    if let Some(t) = o1.kill_time {
        assert!(t > from_millis(7000), "kill time includes travel");
    }
}
