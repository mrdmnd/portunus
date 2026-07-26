//! Kernel-only integration tests (G0): the engine must exercise its whole
//! event loop with no game knowledge — a synthetic in-code actor and
//! [`NoopMechanics`] stand in for codex and mechanics.

use sim_engine::{
    Engine, EngineError, NoopMechanics, ReferenceEngine, RolloutContext, Step, WakeReason,
};
use sim_types::{
    from_millis, AbilityDef, Action, ActorId, CmpOp, CompiledActor, EffectRange, EngageSpec,
    Plan, PredicateExpr, ResolvedCombat, ResolvedRun, ResolvedSpawn, ResourceCost, ResourceDef,
    ResourceId, RunSegment, ScalarRef, Seed, SlotId, TargetSel, WaitSpec,
};

/// A codex-shaped stub with an energy resource and two abilities:
/// slot 0 "poke" (30 energy, instant, on GCD) and slot 1 "burst" (free, 10s cd).
fn stub_actor() -> CompiledActor {
    CompiledActor {
        name: "Stub".into(),
        max_health: 10_000.0,
        gcd: from_millis(1500),
        resources: vec![ResourceDef {
            name: "energy".into(),
            max: 100.0,
            initial: 20.0,
            regen_per_sec: 10.0,
        }],
        abilities: vec![
            AbilityDef {
                name: "poke".into(),
                cost: Some(ResourceCost { resource: ResourceId(0), amount: 30.0 }),
                cooldown: 0,
                cast_time: 0,
                on_gcd: true,
                targeted: true,
                effect: EffectRange::default(),
            },
            AbilityDef {
                name: "burst".into(),
                cost: None,
                cooldown: from_millis(10_000),
                cast_time: 0,
                on_gcd: true,
                targeted: true,
                effect: EffectRange::default(),
            },
        ],
        auras: vec![],
    }
}

/// A stub with a single free ability — no resource crossings, so bare waits
/// sleep until genuinely new events (useful for engagement-timing tests).
fn free_actor() -> CompiledActor {
    CompiledActor {
        name: "Free".into(),
        max_health: 10_000.0,
        gcd: from_millis(1500),
        resources: vec![],
        abilities: vec![AbilityDef {
            name: "tap".into(),
            cost: None,
            cooldown: 0,
            cast_time: 0,
            on_gcd: true,
            targeted: true,
            effect: EffectRange::default(),
        }],
        auras: vec![],
    }
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

fn stub_run(timeout_ms: u64) -> ResolvedRun {
    ResolvedRun::single_combat(ResolvedCombat {
        name: "stub".into(),
        spawns: vec![spawn("dummy#1", 1_000.0, EngageSpec::AtStart)],
        timeout: from_millis(timeout_ms),
    })
}

fn decide(engine: &mut ReferenceEngine<'_, NoopMechanics>) -> sim_engine::DecisionPoint {
    match engine.next_decision() {
        Step::Decide(dp) => dp,
        Step::Done(o) => panic!("combat ended unexpectedly: {o:?}"),
    }
}

const P0: ActorId = ActorId(0);

#[test]
fn combat_starts_with_a_t0_decision_and_masks_are_exact() {
    let roster = [stub_actor()];
    let run = stub_run(60_000);
    let plan = Plan::default();
    let mut engine = ReferenceEngine::new(NoopMechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(1) })
        .unwrap();

    let dp = decide(&mut engine);
    assert_eq!(dp.actor, P0);
    assert_eq!(dp.trigger, WakeReason::CombatStart);
    assert!(!dp.anticipated);
    assert_eq!(engine.now(), 0);

    let mask = engine.legal(P0);
    // 20 energy: poke (30) not castable, exactly 1s away at 10/s regen.
    assert!(!mask.is_castable(SlotId(0)));
    assert!((mask.usable_in[0] - 1.0).abs() < 1e-6);
    // burst is free and off cooldown.
    assert!(mask.is_castable(SlotId(1)));
    assert_eq!(mask.usable_in[1], 0.0);
    assert_eq!(mask.wait_menu.gcd_end, None);
}

/// Decision-point completeness: after a bare Wait(NextEvent), the engine
/// auto-schedules the earliest resource crossing that changes the mask.
#[test]
fn bare_wait_wakes_exactly_at_mask_change() {
    let roster = [stub_actor()];
    let run = stub_run(60_000);
    let plan = Plan::default();
    let mut engine = ReferenceEngine::new(NoopMechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(1) })
        .unwrap();

    decide(&mut engine);
    engine.submit(P0, Action::Wait(WaitSpec::NextEvent)).unwrap();

    let dp = decide(&mut engine);
    assert_eq!(dp.trigger, WakeReason::ResourceCross);
    // Exactly the analytic crossing: (30-20)/10 per sec = 1s.
    assert_eq!(engine.now(), from_millis(1000));
    assert!(engine.legal(P0).is_castable(SlotId(0)));
}

#[test]
fn cast_gcd_and_cooldown_wakes_flow_in_order() {
    let roster = [stub_actor()];
    let run = stub_run(60_000);
    let plan = Plan::default();
    let mut engine = ReferenceEngine::new(NoopMechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(1) })
        .unwrap();

    decide(&mut engine);
    engine
        .submit(P0, Action::Cast { slot: SlotId(1), target: TargetSel::Primary })
        .unwrap();

    // Instant cast completes via a zero-delay event at t=0.
    let dp = decide(&mut engine);
    assert_eq!(dp.trigger, WakeReason::CastEnd);
    assert_eq!(engine.now(), 0);
    let mask = engine.legal(P0);
    assert_eq!(mask.castable_now, 0); // mid-GCD, burst on cd, poke unaffordable
    assert_eq!(mask.wait_menu.gcd_end, Some(from_millis(1500)));

    engine.submit(P0, Action::Wait(WaitSpec::GcdEnd)).unwrap();
    let dp = decide(&mut engine);
    assert_eq!(dp.trigger, WakeReason::GcdEnd);
    assert_eq!(engine.now(), from_millis(1500));

    // Skip forward: burst comes back at exactly 10s.
    engine.submit(P0, Action::Wait(WaitSpec::NextEvent)).unwrap();
    let mut cooldown_wake = None;
    for _ in 0..8 {
        match engine.next_decision() {
            Step::Decide(dp) if dp.trigger == WakeReason::CooldownReady(SlotId(1)) => {
                cooldown_wake = Some(engine.now());
                break;
            }
            Step::Decide(dp) => {
                engine.submit(dp.actor, Action::Wait(WaitSpec::NextEvent)).unwrap();
            }
            Step::Done(o) => panic!("ended early: {o:?}"),
        }
    }
    assert_eq!(cooldown_wake, Some(from_millis(10_000)));
}

#[test]
fn wait_until_and_predicate_wakes_are_anticipated_and_exact() {
    let roster = [stub_actor()];
    let run = stub_run(60_000);
    let plan = Plan::default();
    let mut engine = ReferenceEngine::new(NoopMechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(1) })
        .unwrap();

    decide(&mut engine);
    engine.submit(P0, Action::Wait(WaitSpec::Until(from_millis(2500)))).unwrap();

    // Waits are interruptible leases: the auto-scheduled resource crossing
    // (poke affordable at 1s) legitimately wakes us before the Until fires.
    let dp = decide(&mut engine);
    assert_eq!(dp.trigger, WakeReason::ResourceCross);
    assert_eq!(engine.now(), from_millis(1000));
    engine.submit(P0, Action::Wait(WaitSpec::NextEvent)).unwrap();

    // The original Until lease is still live and fires exactly on time.
    let dp = decide(&mut engine);
    assert_eq!(dp.trigger, WakeReason::WakeAt);
    assert!(dp.anticipated);
    assert_eq!(engine.now(), from_millis(2500));

    // energy >= 80: starts at 20+2.5s*10 = 45; crossing at (80-45)/10 = 3.5s later.
    let pred = engine.compile_predicate(&PredicateExpr::Cmp {
        lhs: ScalarRef::Resource(ResourceId(0)),
        op: CmpOp::Ge,
        rhs: 80.0,
    });
    engine.submit(P0, Action::Wait(WaitSpec::UntilPredicate(pred))).unwrap();
    let dp = decide(&mut engine);
    assert_eq!(dp.trigger, WakeReason::PredicateMet(pred));
    assert!(dp.anticipated);
    assert_eq!(engine.now(), from_millis(6000));
    let energy = engine.state().actors[0].resources[0].value_at(engine.now());
    assert!(energy >= 80.0 - 1e-9);
}

/// Leading travel delays the first combat; a timed wave engages at exactly
/// its offset and produces a SpawnEngaged legality wake.
#[test]
fn travel_then_timed_engagement_wakes_on_time() {
    let roster = [free_actor()];
    let run = ResolvedRun {
        name: "travel+patrol".into(),
        segments: vec![
            RunSegment::Travel { duration: from_millis(5000) },
            RunSegment::Combat(ResolvedCombat {
                name: "pull".into(),
                spawns: vec![
                    spawn("a#1", 1_000.0, EngageSpec::AtStart),
                    spawn("b#1", 1_000.0, EngageSpec::After(from_millis(8000))),
                ],
                timeout: from_millis(60_000),
            }),
        ],
    };
    let plan = Plan::default();
    let mut engine = ReferenceEngine::new(NoopMechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(1) })
        .unwrap();

    // First decision happens after travel, at combat start.
    let dp = decide(&mut engine);
    assert_eq!(dp.trigger, WakeReason::CombatStart);
    assert_eq!(engine.now(), from_millis(5000));
    assert_eq!(engine.state().targetable_enemies().count(), 1);

    engine.submit(P0, Action::Wait(WaitSpec::NextEvent)).unwrap();

    // The body-pull fires at combat start + 8s, exactly.
    let dp = decide(&mut engine);
    assert_eq!(dp.trigger, WakeReason::SpawnEngaged);
    assert_eq!(engine.now(), from_millis(13_000));
    assert_eq!(engine.state().targetable_enemies().count(), 2);
}

#[test]
fn misuse_is_an_error_not_a_panic() {
    let roster = [stub_actor()];
    let run = stub_run(60_000);
    let plan = Plan::default();
    let mut engine = ReferenceEngine::new(NoopMechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(1) })
        .unwrap();

    // Submit before any decision was handed out.
    assert!(matches!(
        engine.submit(P0, Action::Wait(WaitSpec::NextEvent)),
        Err(EngineError::NoPendingDecision(_))
    ));

    decide(&mut engine);
    // Wrong actor.
    assert!(matches!(
        engine.submit(ActorId(1), Action::Wait(WaitSpec::NextEvent)),
        Err(EngineError::WrongActor { .. })
    ));
    // Unaffordable cast (poke at 20 energy).
    assert!(matches!(
        engine.submit(P0, Action::Cast { slot: SlotId(0), target: TargetSel::Primary }),
        Err(EngineError::IllegalAction { .. })
    ));
    // GcdEnd wait with no GCD running.
    assert!(matches!(
        engine.submit(P0, Action::Wait(WaitSpec::GcdEnd)),
        Err(EngineError::IllegalAction { .. })
    ));
    // The decision is still owed after failed submits; a legal wait works.
    engine.submit(P0, Action::Wait(WaitSpec::NextEvent)).unwrap();
}

/// A policy that spins on an always-true predicate never advances time; the
/// engine detects the livelock instead of hanging.
#[test]
fn same_timestamp_wait_spin_is_detected() {
    let roster = [stub_actor()];
    let run = stub_run(60_000);
    let plan = Plan::default();
    let mut engine = ReferenceEngine::new(NoopMechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(1) })
        .unwrap();

    let always_true = engine.compile_predicate(&PredicateExpr::Cmp {
        lhs: ScalarRef::TimeSeconds,
        op: CmpOp::Ge,
        rhs: 0.0,
    });

    let mut result = Ok(());
    for _ in 0..1000 {
        let dp = decide(&mut engine);
        result = engine.submit(dp.actor, Action::Wait(WaitSpec::UntilPredicate(always_true)));
        if result.is_err() {
            break;
        }
    }
    assert_eq!(result, Err(EngineError::Livelock(P0)));
    assert_eq!(engine.now(), 0, "livelock never advanced time");
}

/// NoopMechanics never kills the dummy: the rollout must terminate at exactly
/// the pull timeout as a wipe, and identically across two runs.
#[test]
fn timeout_ends_combat_deterministically() {
    let roster = [stub_actor()];
    let run = stub_run(10_000);
    let plan = Plan::default();

    let go = || {
        let mut engine = ReferenceEngine::new(NoopMechanics);
        engine
            .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(7) })
            .unwrap();
        loop {
            match engine.next_decision() {
                Step::Decide(dp) => {
                    let mask = engine.legal(dp.actor);
                    // Cast whatever is legal, else wait.
                    let action = (0..2)
                        .map(SlotId)
                        .find(|s| mask.is_castable(*s))
                        .map(|slot| Action::Cast { slot, target: TargetSel::Primary })
                        .unwrap_or(Action::Wait(WaitSpec::NextEvent));
                    engine.submit(dp.actor, action).unwrap();
                }
                Step::Done(o) => return (o, engine.trace_hash()),
            }
        }
    };

    let (o1, h1) = go();
    let (o2, h2) = go();
    assert_eq!(o1.kill_time, None, "noop mechanics cannot kill");
    assert_eq!(o1, o2, "same seed, same outcome (invariant 1)");
    assert_eq!(h1, h2, "same seed, same event trace");
}

/// Snapshot mid-rollout, run to completion, restore, run again: identical
/// outcome and trace (invariant 7).
#[test]
fn snapshot_restore_replays_identically() {
    let roster = [stub_actor()];
    let run = stub_run(15_000);
    let plan = Plan::default();
    let mut engine = ReferenceEngine::new(NoopMechanics);
    engine
        .reset(RolloutContext { run: &run, roster: &roster, plan: &plan, seed: Seed(3) })
        .unwrap();

    let drive = |engine: &mut ReferenceEngine<'_, NoopMechanics>, max_steps: usize| {
        for _ in 0..max_steps {
            match engine.next_decision() {
                Step::Decide(dp) => {
                    let mask = engine.legal(dp.actor);
                    let action = if mask.is_castable(SlotId(0)) {
                        Action::Cast { slot: SlotId(0), target: TargetSel::Primary }
                    } else {
                        Action::Wait(WaitSpec::NextEvent)
                    };
                    engine.submit(dp.actor, action).unwrap();
                }
                Step::Done(o) => return Some(o),
            }
        }
        None
    };

    drive(&mut engine, 6);
    let snap = engine.snapshot();
    let t_snap = engine.now();

    let first = drive(&mut engine, 10_000).expect("must finish");
    let h_first = engine.trace_hash();

    engine.restore(&snap);
    assert_eq!(engine.now(), t_snap);
    let second = drive(&mut engine, 10_000).expect("must finish");
    let h_second = engine.trace_hash();

    assert_eq!(first, second);
    assert_eq!(h_first, h_second);
}
