//! The checked-in run files, end to end.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{HeroTreeId, PetId, Seed, SimDuration, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, MoveGoal, Readiness, StateView, TargetSel, Wait, WakeReason,
};
use portunus_env::{Decision, Env, Turn};
use portunus_eval::{Arm, LocalRunner, Metric, Runner, SeedSet};
use portunus_gamedata::spell::CastKind;
use portunus_run::{Bundle, ChannelObs, EmpowerObs, RunError, SeatObs};

const LIGHTNING_BOLT: SpellId = SpellId(188196);

fn bundle() -> Bundle {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs/elemental_dummy.ron");
    Bundle::load(&path).unwrap()
}

#[test]
fn experiment_runs_clean() {
    let b = bundle();
    let seeds = SeedSet {
        first: 0,
        count: 16,
    };
    let report = LocalRunner {
        threads: NonZeroUsize::new(4).unwrap(),
    }
    .run(&b.experiment(seeds));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let metric = |m: Metric| {
        report.arms[0]
            .metrics
            .iter()
            .find(|(x, _)| *x == m)
            .unwrap()
            .1
    };
    assert_eq!(metric(Metric::CompletionRate).mean, 1.0);
    assert!(metric(Metric::SeatDps(portunus_core::Seat(0))).mean > 0.0);
}

#[test]
fn breakdown_accounts_for_all_damage() {
    let b = bundle();
    let seeds = SeedSet { first: 0, count: 4 };
    let bd = b.breakdown(seeds).unwrap();
    let arm = b.arm(false);
    let mean_damage: f64 = seeds
        .iter()
        .map(|s| arm.rollout(s).unwrap().seats[0].damage_done as f64)
        .sum::<f64>()
        / 4.0;
    let total: f64 = bd.rows.iter().map(|r| r.damage).sum();
    assert!((total - mean_damage).abs() < 1e-6 * mean_damage);
    let share: f64 = bd.rows.iter().map(|r| r.share).sum();
    assert!((share - 1.0).abs() < 1e-9);
    assert!(bd
        .rows
        .iter()
        .any(|r| r.name == "Lava Burst" && r.crit_rate > 0.9));
}

#[test]
fn rollouts_repeat_and_forks_agree() {
    let b = bundle();
    let arm = b.arm(false);
    assert_eq!(arm.rollout(Seed(9)).unwrap(), arm.rollout(Seed(9)).unwrap());

    let policy = &arm.policies[0];
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(9)).unwrap();
    for _ in 0..40 {
        let Turn::Decide(d) = turn else {
            panic!("ended early")
        };
        turn = env.step(policy.act(&d)).unwrap().next;
    }
    let finish = |mut env: portunus_run::SimEnv<_>, mut turn| loop {
        match turn {
            Turn::Done(o) => return o,
            Turn::Decide(d) => turn = env.step(policy.act(&d)).unwrap().next,
        }
    };
    let fork = finish(env.clone(), turn.clone());
    let main = finish(env, turn);
    assert_eq!(fork, main);
    assert_eq!(main, arm.rollout(Seed(9)).unwrap());
}

/// A hero tree's run file: forks agree, and policies see the tree.
fn hero_forks_agree(run: &str, tree: HeroTreeId) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/runs")
        .join(run);
    let b = Bundle::load(&path).unwrap();
    let arm = b.arm(false);
    let policy = &arm.policies[0];
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(3)).unwrap();
    for _ in 0..20 {
        let Turn::Decide(d) = turn else {
            panic!("ended early")
        };
        assert_eq!(d.obs.hero_tree, Some(tree));
        turn = env.step(policy.act(&d)).unwrap().next;
    }
    let finish = |mut env: portunus_run::SimEnv<_>, mut turn| loop {
        match turn {
            Turn::Done(o) => return o,
            Turn::Decide(d) => turn = env.step(policy.act(&d)).unwrap().next,
        }
    };
    let fork = finish(env.clone(), turn.clone());
    let main = finish(env, turn);
    assert_eq!(fork, main);
    assert_eq!(main, arm.rollout(Seed(3)).unwrap());
}

#[test]
fn stormbringer_forks_agree_and_policies_see_the_tree() {
    hero_forks_agree("elemental_stormbringer_dummy.ron", HeroTreeId(54));
}

#[test]
fn farseer_forks_agree_and_policies_see_the_tree() {
    hero_forks_agree("elemental_farseer_dummy.ron", HeroTreeId(55));
}

/// The Ancestors' damage is the seat's, and the breakdown credits it to them.
#[test]
fn breakdown_credits_the_ancestors() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/runs/elemental_farseer_dummy.ron");
    let b = Bundle::load(&path).unwrap();
    let seeds = SeedSet { first: 0, count: 4 };
    let bd = b.breakdown(seeds).unwrap();
    let arm = b.arm(false);
    let mean_damage: f64 = seeds
        .iter()
        .map(|s| arm.rollout(s).unwrap().seats[0].damage_done as f64)
        .sum::<f64>()
        / 4.0;
    let total: f64 = bd.rows.iter().map(|r| r.damage).sum();
    assert!((total - mean_damage).abs() < 1e-6 * mean_damage);
    let ancestors = bd
        .rows
        .iter()
        .find(|r| r.pet == Some(PetId(221177)))
        .expect("an Ancestor row");
    assert_eq!(ancestors.name, "Ancestor: Lava Burst");
    assert!(ancestors.casts > 0.0 && ancestors.crit_rate == 1.0);
}

/// The full builds' rotations press every button they talent, and their
/// Elementals come out primal.
#[test]
fn full_builds_press_their_whole_kit() {
    let runs: [(&str, &[&str]); 2] = [
        (
            "elemental_stormbringer_dummy",
            &[
                "Lightning Shield",
                "Stormkeeper",
                "Ascendance",
                "Voltaic Blaze",
                "Lava Burst",
                "Earth Shock",
                "Lightning Bolt",
                "Tempest",
            ],
        ),
        (
            "elemental_farseer_dummy",
            &[
                "Lightning Shield",
                "Stormkeeper",
                "Ancestral Swiftness",
                "Ascendance",
                "Voltaic Blaze",
                "Lava Burst",
                "Elemental Blast",
                "Lightning Bolt",
            ],
        ),
    ];
    for (run, presses) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in presses {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.pet.is_none() && r.name == *name && r.casts > 0.0),
                "{run} casts {name}"
            );
        }
        assert!(
            bd.rows
                .iter()
                .any(|r| r.name == "Primal Fire Elemental: Meteor"),
            "{run}: Call of Fire's Elemental"
        );
    }
}

/// Lava Surge's buff and Lava Burst's reset stay hidden until perceived.
#[test]
fn realistic_seats_miss_what_they_have_not_perceived() {
    let b = bundle();
    let arm = b.arm(false);
    let policy = &arm.policies[0];
    let mut hidden_seen = 0;
    for seed in 0..20 {
        let mut env = b.env(false);
        let mut turn = env.reset(Seed(seed)).unwrap();
        while let Turn::Decide(d) = turn {
            let state = env.state().unwrap();
            for p in state.unperceived(d.seat) {
                match p.reason {
                    WakeReason::AuraGained(a) => {
                        assert!(!d.obs.buffs.contains_key(&a));
                        hidden_seen += 1;
                    }
                    WakeReason::CooldownReady(s) => {
                        assert_ne!(d.legal.abilities.get(&s), Some(&Readiness::Now));
                        hidden_seen += 1;
                    }
                    _ => {}
                }
            }
            turn = env.step(policy.act(&d)).unwrap().next;
        }
    }
    assert!(hidden_seen > 0);
}

/// The script dodges every swirl and quake, uses its movement tools, and
/// chases the dummy back into range after its leap.
#[test]
fn elemental_meets_the_mechanics_dummys_demands() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/runs/elemental_stormbringer_mechanics.ron");
    let b = Bundle::load(&path).unwrap();
    let policy = &b.arm(false).policies[0];
    let mut env = b.env(true);
    let mut turn = env.reset(Seed(2)).unwrap();
    let outcome = loop {
        match turn {
            Turn::Done(o) => break o,
            Turn::Decide(d) => turn = env.step(policy.act(&d)).unwrap().next,
        }
    };
    let trace = env.drain_trace();
    let count = |pick: fn(&TraceEvent) -> bool| trace.iter().filter(|r| pick(&r.event)).count();

    assert!(outcome.completed);
    let placed = count(|e| matches!(e, TraceEvent::Demand { .. }));
    assert!(placed >= 10, "{placed} demands");
    assert_eq!(count(|e| matches!(e, TraceEvent::DemandMet { .. })), placed);
    assert_eq!(outcome.seats[0].demands_failed, 0);
    let far = count(|e| matches!(e, TraceEvent::Distance { distance, .. } if *distance > 40.0));
    assert!(far >= 3, "{far} leaps");
    let chased = trace.iter().any(|r| {
        matches!(
            &r.event,
            TraceEvent::Decision {
                choice: Choice::Move(MoveGoal::Approach { .. }),
                ..
            }
        )
    });
    assert!(chased, "approaches after a leap");
}

#[test]
fn seats_see_their_channels_progress() {
    let mut b = bundle();
    let bolt = Arc::make_mut(&mut b.data)
        .spells
        .get_mut(&LIGHTNING_BOLT)
        .unwrap();
    bolt.cast = CastKind::Channel {
        duration: SimDuration(3000),
        ticks: 3,
        hasted: false,
        swings: false,
    };
    bolt.speed = None;
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut seen = Vec::new();
    while let Turn::Decide(d) = turn {
        let choice = if let Some(c) = d.obs.channel {
            if d.reason == WakeReason::ChannelTick {
                seen.push(c);
            }
            Choice::Wait(Wait::NextEvent)
        } else if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::Cast {
                ability: LIGHTNING_BOLT,
                target: TargetSel::Primary,
                opts: CastOpts {
                    empower: None,
                    tick_wakes: true,
                },
            }
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        if seen.len() == 2 {
            break;
        }
        turn = env.step(choice).unwrap().next;
    }
    let tick = |ticks_done| ChannelObs {
        ticks_done,
        ticks_total: 3,
        next_tick: SimDuration(1000),
    };
    assert_eq!(seen, vec![tick(1), tick(2)]);
}

#[test]
fn seats_see_their_empowers_progress() {
    let mut b = bundle();
    let bolt = Arc::make_mut(&mut b.data)
        .spells
        .get_mut(&LIGHTNING_BOLT)
        .unwrap();
    bolt.cast = CastKind::Empower {
        stages: vec![SimDuration(1000), SimDuration(1750)],
        hasted: false,
        hold: SimDuration(2000),
        stage_effects: vec![Vec::new(), Vec::new()],
    };
    bolt.speed = None;
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut seen = Vec::new();
    while let Turn::Decide(d) = turn {
        let choice = if let Some(e) = d.obs.empower {
            if matches!(d.reason, WakeReason::EmpowerStage(_)) {
                seen.push(e);
            }
            Choice::Wait(Wait::NextEvent)
        } else if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::Cast {
                ability: LIGHTNING_BOLT,
                target: TargetSel::Primary,
                opts: CastOpts {
                    empower: None,
                    tick_wakes: true,
                },
            }
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        if seen.len() == 2 {
            break;
        }
        turn = env.step(choice).unwrap().next;
    }
    let stage = |stage, next_stage| EmpowerObs { stage, next_stage };
    assert_eq!(seen, vec![stage(1, Some(SimDuration(750))), stage(2, None)]);
}

#[test]
fn closures_are_policies() {
    let b = bundle();
    let bolt_only = |d: &Decision<SeatObs>| {
        if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::cast(LIGHTNING_BOLT)
        } else {
            Choice::Wait(Wait::NextEvent)
        }
    };
    let mut arm = b.arm(false);
    let full = arm.rollout(Seed(1)).unwrap();
    arm.policies = vec![Arc::new(bolt_only)];
    let bolts = arm.rollout(Seed(1)).unwrap();
    assert!(bolts.completed);
    assert!(bolts.end_time > full.end_time);
}

#[test]
fn unknown_policies_are_rejected() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs");
    let mut config = bundle().config;
    config.seats[0].policy = "nope".into();
    assert!(matches!(
        Bundle::from_config(config, &path),
        Err(RunError::UnknownPolicy { seat: 0, .. })
    ));
}
