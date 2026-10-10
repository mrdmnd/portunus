mod common;

use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, EnemyKey, EventName, SimDuration, SimTime, Trigger};
use portunus_engine::trace::{TraceEvent, TraceRecord};
use portunus_engine::{
    Choice, Engine, EngineError, Kernel, SetupIssue, StateView, Step, Wait, WakeReason,
};
use portunus_gamedata::enemy::{
    EnemyAction, EnemyKind, EnemyRule, EnemySubject, EnemyTarget, PhaseName,
};
use portunus_gamedata::stats::SchoolMask;
use portunus_scenario::{Sampler, ScenarioSampler};

use common::*;

const SPAWN_AT: u32 = 10_000;
const ADD_HEALTH: u64 = 40_000;
const ADD_FORCES: u32 = 3;
/// Bloodlust, made permanent here: a buff with nothing periodic.
const FRENZY: AuraId = AuraId(2825);

fn dummy() -> EnemyKey {
    EnemyKey("target_dummy".into())
}

fn imp() -> EnemyKey {
    EnemyKey("imp".into())
}

/// The dummy spawns two imps `SPAWN_AT` into the pull. Each imp bites the
/// tank for one point every second, frenzies at once, and howls below half
/// health.
fn spawning(distance: Option<Dist<f64>>, despawn_with_spawner: bool) -> Fixture {
    let mut f = fixture();
    let mut data = (*f.data).clone();
    data.auras
        .get_mut(&FRENZY)
        .expect("Bloodlust exists")
        .duration = None;
    f.data = Arc::new(data);
    let mut enemies = (*f.enemies).clone();
    let mut add = enemies.enemies[&dummy()].clone();
    add.key = imp();
    add.name = "Imp".into();
    add.kind = EnemyKind::Add;
    add.health = ADD_HEALTH;
    add.forces = ADD_FORCES;
    add.rules = vec![
        EnemyRule {
            name: EventName("bite".into()),
            phase: None,
            when: Trigger::Now,
            repeat: Some(Dist::Fixed(SimDuration(1000))),
            action: EnemyAction::Damage {
                amount: Dist::Fixed(1.0),
                school: SchoolMask::PHYSICAL,
                target: EnemyTarget::Tank,
            },
        },
        EnemyRule {
            name: EventName("howl".into()),
            phase: None,
            when: Trigger::HpFracBelow {
                who: EnemySubject::Itself,
                frac: 0.5,
            },
            repeat: None,
            action: EnemyAction::EnterPhase(PhaseName("howling".into())),
        },
        EnemyRule {
            name: EventName("frenzy".into()),
            phase: None,
            when: Trigger::Now,
            repeat: None,
            action: EnemyAction::SelfAura(FRENZY),
        },
    ];
    enemies.enemies.insert(imp(), add);
    enemies
        .enemies
        .get_mut(&dummy())
        .expect("the dummy exists")
        .rules = vec![EnemyRule {
        name: EventName("summon".into()),
        phase: None,
        when: Trigger::<EnemySubject>::Elapsed(SimDuration(SPAWN_AT)),
        repeat: None,
        action: EnemyAction::SpawnAdds {
            adds: vec![(imp(), 2)],
            distance,
            despawn_with_spawner,
        },
    }];
    f.enemies = Arc::new(enemies);
    let mut spec = f.sampler.spec().clone();
    spec.pulls[0].health = Dist::Fixed(1.0);
    f.sampler = Sampler::new(spec, Arc::clone(&f.enemies)).unwrap();
    f
}

fn times(trace: &[TraceRecord], pick: impl Fn(&TraceEvent) -> bool) -> Vec<SimTime> {
    trace
        .iter()
        .filter(|r| pick(&r.event))
        .map(|r| r.time)
        .collect()
}

fn spawned(trace: &[TraceRecord]) -> Vec<(SimTime, ActorId, ActorId)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Spawn { actor, spawner } => Some((r.time, actor, spawner)),
            _ => None,
        })
        .collect()
}

/// Play until the adds are in, returning the kernel paused at the first
/// decision after.
fn until_spawned(k: &mut Kernel<Stub>) {
    for _ in 0..100_000 {
        if k.state().enemies().len() > 1 {
            return;
        }
        match k.advance().unwrap() {
            Step::Done(o) => panic!("run ended before the adds: {o:?}"),
            Step::Decide(req) => {
                let choice = priority(k, req.seat);
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    panic!("no adds");
}

#[test]
fn adds_fight_and_must_die_for_the_pull_to_clear() {
    let f = spawning(None, false);
    let mut k = kernel(&f, 1, 1, instant());
    let outcome = play(&mut k);
    let trace = k.drain_trace();
    assert!(outcome.completed, "{outcome:?}");
    let state = k.state();
    let boss = state.enemies()[0];
    let me = state.seats()[0];

    let engaged = times(
        &trace,
        |e| matches!(e, TraceEvent::Engage { actor } if *actor == boss),
    );
    let adds = spawned(&trace);
    assert_eq!(adds.len(), 2);
    let ids: Vec<ActorId> = adds.iter().map(|a| a.1).collect();
    assert_eq!(&state.enemies()[1..], ids.as_slice());
    for &(at, add, spawner) in &adds {
        assert_eq!(at, engaged[0] + SimDuration(SPAWN_AT));
        assert_eq!(spawner, boss);
        let view = state.actor(add).unwrap();
        assert_eq!(view.max_health, ADD_HEALTH);
        assert!(!view.alive);
        let info = state.enemy_info(add).unwrap();
        assert_eq!(info.key, &imp());
        assert_eq!(info.spawner, Some(boss));
        assert_eq!(info.forces, ADD_FORCES);
        assert!(!info.despawned);
        // Engaged on arrival, so their own rules ran at once.
        let engage = times(
            &trace,
            |e| matches!(e, TraceEvent::Engage { actor } if *actor == add),
        );
        assert_eq!(engage, vec![at]);
        let bites = times(
            &trace,
            |e| matches!(e, TraceEvent::EnemyRule { actor, .. } if *actor == add),
        );
        assert_eq!(bites.first(), Some(&at));
        assert!(trace.iter().any(|r| matches!(&r.event,
            TraceEvent::Damage(d) if d.source == add && d.target == me)));
        let hits: u64 = trace
            .iter()
            .filter_map(|r| match &r.event {
                TraceEvent::Damage(d) if d.target == add && d.source == me => Some(d.amount),
                _ => None,
            })
            .sum();
        assert!(hits >= ADD_HEALTH, "the seat killed it");
        // Its own health, not the boss's, sets off its howl.
        let half = trace
            .iter()
            .scan(0, |taken, r| {
                if let TraceEvent::Damage(d) = &r.event {
                    if d.target == add {
                        *taken += d.amount;
                    }
                }
                Some((r.time, *taken))
            })
            .find(|&(_, taken)| 2 * taken > ADD_HEALTH)
            .map(|(t, _)| t);
        let howl = times(
            &trace,
            |e| matches!(e, TraceEvent::EnemyRule { actor, rule } if *actor == add && rule.0 == 1),
        );
        assert_eq!(howl.first().copied(), half);
    }
    assert!(state.enemy_info(boss).unwrap().spawner.is_none());

    let deaths = |id: ActorId| {
        times(
            &trace,
            |e| matches!(e, TraceEvent::Death { actor } if *actor == id),
        )
    };
    let boss_died = deaths(boss)[0];
    let last = ids.iter().map(|&a| deaths(a)[0]).max().unwrap();
    // The seat stays on the boss, so the adds outlive it.
    assert!(last > boss_died);
    assert_eq!(outcome.pulls[0].cleared, Some(last));
}

#[test]
fn seats_hear_of_adds_and_see_them_at_their_spawner() {
    let f = spawning(None, false);
    let mut k = kernel(&f, 1, 1, instant());
    until_spawned(&mut k);
    let state = k.state();
    let boss = state.enemies()[0];
    let seat = portunus_core::Seat(0);
    for &add in &state.enemies()[1..] {
        assert_eq!(state.distance(seat, add), state.distance(seat, boss));
    }

    // A seat waiting on nothing in particular is woken by each add.
    let mut k = kernel(&f, 1, 1, instant());
    let mut reasons = Vec::new();
    while k.state().enemies().len() < 2 {
        match k.advance().unwrap() {
            Step::Done(o) => panic!("run ended before the adds: {o:?}"),
            Step::Decide(req) => {
                reasons.push(req.reason);
                k.submit(req.seat, Choice::Wait(Wait::NextEvent)).unwrap();
            }
        }
    }
    let adds = &k.state().enemies()[1..];
    assert!(
        matches!(reasons.last(), Some(WakeReason::EnemyEngaged(a)) if adds.contains(a)),
        "{reasons:?}"
    );
}

#[test]
fn adds_can_spawn_at_their_own_distance_and_share_the_pull_health() {
    let mut f = spawning(Some(Dist::Fixed(25.0)), false);
    let mut spec = f.sampler.spec().clone();
    spec.pulls[0].health = Dist::Fixed(1.5);
    f.sampler = Sampler::new(spec, Arc::clone(&f.enemies)).unwrap();
    let mut k = kernel(&f, 1, 1, instant());
    until_spawned(&mut k);
    let state = k.state();
    for &add in &state.enemies()[1..] {
        assert_eq!(state.distance(portunus_core::Seat(0), add), Some(25.0));
        assert_eq!(state.actor(add).unwrap().max_health, ADD_HEALTH * 3 / 2);
    }
}

#[test]
fn adds_can_leave_with_their_spawner() {
    let f = spawning(None, true);
    let mut k = kernel(&f, 1, 1, instant());
    let outcome = play(&mut k);
    let trace = k.drain_trace();
    assert!(outcome.completed, "{outcome:?}");
    let state = k.state();
    let boss = state.enemies()[0];
    let ids: Vec<ActorId> = spawned(&trace).iter().map(|a| a.1).collect();
    assert_eq!(ids.len(), 2);

    let boss_died = times(
        &trace,
        |e| matches!(e, TraceEvent::Death { actor } if *actor == boss),
    )[0];
    for &add in &ids {
        let gone = times(
            &trace,
            |e| matches!(e, TraceEvent::Despawn { actor } if *actor == add),
        );
        assert_eq!(gone, vec![boss_died]);
        assert!(!trace
            .iter()
            .any(|r| matches!(r.event, TraceEvent::Death { actor } if actor == add)));
        assert!(!state.actor(add).unwrap().alive);
        assert!(state.enemy_info(add).unwrap().despawned);
        let frenzied = times(
            &trace,
            |e| matches!(e, TraceEvent::AuraApplied { holder, aura, .. } if *holder == add && *aura == FRENZY),
        );
        assert_eq!(frenzied.len(), 1);
        assert!(state.auras(add).is_empty());
        let bites = times(
            &trace,
            |e| matches!(e, TraceEvent::EnemyRule { actor, .. } if *actor == add),
        );
        assert!(bites.iter().all(|&t| t <= boss_died), "it stopped acting");
    }
    assert_eq!(outcome.pulls[0].cleared, Some(boss_died));
}

#[test]
fn spawned_enemies_must_exist() {
    let mut f = spawning(None, false);
    let mut enemies = (*f.enemies).clone();
    let rules = &mut enemies.enemies.get_mut(&imp()).unwrap().rules;
    rules.push(EnemyRule {
        name: EventName("more".into()),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::SpawnAdds {
            adds: vec![(EnemyKey("ghost".into()), 1)],
            distance: None,
            despawn_with_spawner: false,
        },
    });
    f.enemies = Arc::new(enemies);
    let Err(EngineError::Setup(issues)) = Kernel::new(setup(&f, 1, 1, instant()), Stub) else {
        panic!("expected a setup error");
    };
    assert_eq!(
        issues,
        vec![SetupIssue::UnknownEnemy(EnemyKey("ghost".into()))]
    );
}
