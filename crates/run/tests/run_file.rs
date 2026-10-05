//! The checked-in run file, end to end.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{Seed, SpellId};
use portunus_engine::{Choice, Readiness, StateView, Wait, WakeReason};
use portunus_env::{Decision, Env, Turn};
use portunus_eval::{Arm, LocalRunner, Metric, Runner, SeedSet};
use portunus_run::{Bundle, RunError, SeatObs};

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
        .map(|s| arm.rollout(s).unwrap().seats[0].damage_done)
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
