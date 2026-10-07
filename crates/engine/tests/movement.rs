mod common;

use std::sync::Arc;

use portunus_core::{Dist, EnemyKey, EventName, SimDuration, SimTime, Trigger};
use portunus_engine::trace::{CastEndReason, TraceEvent, TraceRecord};
use portunus_engine::{
    Choice, DecisionRequest, Engine, EngineError, IllegalChoice, Kernel, MoveGoal, Outcome,
    Readiness, StateView, Step, Wait, WakeReason,
};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemySubject, EnemyTarget};
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::stats::SchoolMask;
use portunus_scenario::{Sampler, ScenarioSampler};

use common::*;

/// The fixture with the target dummy running `rules`.
fn with_rules(rules: Vec<EnemyRule>) -> Fixture {
    let mut f = fixture();
    let mut enemies = (*f.enemies).clone();
    let dummy = enemies
        .enemies
        .get_mut(&EnemyKey("target_dummy".into()))
        .expect("the dummy exists");
    dummy.rules = rules;
    f.enemies = Arc::new(enemies);
    f.sampler = Sampler::new(f.sampler.spec().clone(), Arc::clone(&f.enemies)).unwrap();
    f
}

fn rule(name: &str, when: Trigger<EnemySubject>, action: EnemyAction) -> EnemyRule {
    EnemyRule {
        name: EventName(name.into()),
        phase: None,
        when,
        repeat: None,
        action,
    }
}

fn at(ms: u32) -> Trigger<EnemySubject> {
    Trigger::Elapsed(SimDuration(ms))
}

fn flat(amount: f64) -> Effect {
    Effect::Damage {
        amount: Coefficient::Flat(amount),
        school: SchoolMask::PHYSICAL,
        target: EffectTarget::Target,
        aoe: None,
    }
}

/// Answer every decision with `answer` until the run ends.
fn drive(
    k: &mut Kernel<Stub>,
    mut answer: impl FnMut(&Kernel<Stub>, &DecisionRequest) -> Choice,
) -> Outcome {
    for _ in 0..100_000 {
        match k.advance().unwrap() {
            Step::Done(o) => return o,
            Step::Decide(req) => {
                let choice = answer(k, &req);
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    panic!("the run should end");
}

fn engaged_at(trace: &[TraceRecord]) -> SimTime {
    trace
        .iter()
        .find(|r| matches!(r.event, TraceEvent::Engage { .. }))
        .expect("the dummy engages")
        .time
}

fn times(trace: &[TraceRecord], pick: impl Fn(&TraceEvent) -> bool) -> Vec<SimTime> {
    trace
        .iter()
        .filter(|r| pick(&r.event))
        .map(|r| r.time)
        .collect()
}

#[test]
fn rules_fire_on_the_enemys_clock_and_repeat() {
    let mut hit = rule(
        "hit",
        at(2000),
        EnemyAction::Damage {
            amount: Dist::Fixed(100.0),
            school: SchoolMask::PHYSICAL,
            target: EnemyTarget::Tank,
        },
    );
    hit.repeat = Some(Dist::Fixed(SimDuration(5000)));
    let f = with_rules(vec![hit]);
    let mut k = kernel(&f, 1, 1, instant());
    let outcome = play(&mut k);
    let trace = k.drain_trace();

    let start = engaged_at(&trace);
    let fired = times(&trace, |e| matches!(e, TraceEvent::EnemyRule { .. }));
    assert!(fired.len() > 3, "{fired:?}");
    assert_eq!(fired[0], start + SimDuration(2000));
    for pair in fired.windows(2) {
        assert_eq!(pair[1] - pair[0], SimDuration(5000));
    }
    // With no tank, the first living seat takes tank hits.
    assert_eq!(outcome.seats[0].damage_taken, 100 * fired.len() as u64);
    assert_eq!(outcome.seats[0].deaths, 0);
}

#[test]
fn delayed_triggers_count_from_another_rules_firing() {
    let first = rule(
        "first",
        at(1000),
        EnemyAction::Effects {
            effects: vec![],
            target: EnemyTarget::AllPlayers,
        },
    );
    let second = rule(
        "second",
        Trigger::Delayed {
            delay: SimDuration(3000),
            after: Box::new(Trigger::Fired {
                who: EnemySubject::Itself,
                event: EventName("first".into()),
                nth: 1,
            }),
        },
        EnemyAction::Effects {
            effects: vec![flat(500.0)],
            target: EnemyTarget::AllPlayers,
        },
    );
    let f = with_rules(vec![first, second]);
    let mut k = kernel(&f, 2, 1, instant());
    let outcome = play(&mut k);
    let trace = k.drain_trace();

    let start = engaged_at(&trace);
    let fired: Vec<(SimTime, u16)> = trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::EnemyRule { rule, .. } => Some((r.time, rule.0)),
            _ => None,
        })
        .collect();
    assert_eq!(
        fired,
        vec![
            (start + SimDuration(1000), 0),
            (start + SimDuration(4000), 1)
        ]
    );
    assert_eq!(outcome.seats[0].damage_taken, 500);
}

#[test]
fn enemy_casts_land_when_the_bar_fills() {
    let slam = rule(
        "slam",
        at(1000),
        EnemyAction::Cast {
            time: SimDuration(2500),
            interruptible: true,
            then: Box::new(EnemyAction::Effects {
                effects: vec![flat(700.0)],
                target: EnemyTarget::AllPlayers,
            }),
        },
    );
    let f = with_rules(vec![slam]);
    let mut k = kernel(&f, 3, 1, instant());
    let outcome = play(&mut k);
    let trace = k.drain_trace();

    let start = engaged_at(&trace);
    let began = times(&trace, |e| matches!(e, TraceEvent::EnemyCastStart { .. }));
    let ended = times(&trace, |e| {
        matches!(
            e,
            TraceEvent::EnemyCastEnd {
                reason: CastEndReason::Completed,
                ..
            }
        )
    });
    assert_eq!(began, vec![start + SimDuration(1000)]);
    assert_eq!(ended, vec![start + SimDuration(3500)]);
    let hit = trace
        .iter()
        .find(|r| matches!(&r.event, TraceEvent::Damage(d) if d.amount == 700))
        .expect("the payload lands");
    assert_eq!(hit.time, start + SimDuration(3500));
    assert_eq!(outcome.seats[0].damage_taken, 700);
}

#[test]
fn forced_movement_interrupts_hard_casts() {
    let push = rule(
        "push",
        at(2000),
        EnemyAction::ForceMovement {
            duration: Dist::Fixed(SimDuration(3000)),
            target: EnemyTarget::AllPlayers,
        },
    );
    let f = with_rules(vec![push]);
    let mut k = kernel(&f, 4, 1, instant());
    play(&mut k);
    let trace = k.drain_trace();

    let start = engaged_at(&trace);
    let pushed = start + SimDuration(2000);
    let moved: Vec<(SimTime, SimTime)> = trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::MovementStart {
                ends, forced: true, ..
            } => Some((r.time, ends)),
            _ => None,
        })
        .collect();
    assert_eq!(moved, vec![(pushed, pushed + SimDuration(3000))]);
    // Flame Shock at the pull, then Lava Burst from 1.5 s: cut off at 2 s.
    let interrupted = times(&trace, |e| {
        matches!(
            e,
            TraceEvent::CastEnd {
                spell: LAVA_BURST,
                reason: CastEndReason::Interrupted,
                ..
            }
        )
    });
    assert_eq!(interrupted, vec![pushed]);
    for r in &trace {
        if let TraceEvent::CastStart { spell, .. } = r.event {
            let hard = !matches!(f.data.spells[&spell].cast, CastKind::Instant);
            let during = r.time >= pushed && r.time < pushed + SimDuration(3000);
            assert!(!(hard && during), "hard cast while moving: {r:?}");
        }
    }
}

fn swirl(on_fail: Vec<Effect>) -> EnemyRule {
    rule(
        "swirl",
        at(3000),
        EnemyAction::MustMove {
            yards: 8.0,
            within: SimDuration(1500),
            target: EnemyTarget::AllPlayers,
            on_fail,
        },
    )
}

#[test]
fn moving_in_time_meets_a_demand() {
    let f = with_rules(vec![swirl(vec![flat(5000.0)])]);
    let mut k = kernel(&f, 5, 1, instant());
    let mut asked = None;
    let mut moved_twice = false;
    let outcome = drive(&mut k, |k, req| {
        let state = k.state();
        let idle = state.movement(req.seat).is_none();
        if !idle && !state.demands(req.seat).is_empty() {
            let again = k
                .clone()
                .submit(req.seat, Choice::Move(MoveGoal::ClearDemands));
            assert!(matches!(
                again,
                Err(EngineError::Illegal {
                    reason: IllegalChoice::AlreadyMoving,
                    ..
                })
            ));
            moved_twice = true;
        }
        if idle && !state.demands(req.seat).is_empty() && k.legal(req.seat).can_move {
            asked.get_or_insert(req.reason);
            Choice::Move(MoveGoal::ClearDemands)
        } else {
            priority(k, req.seat)
        }
    });
    let trace = k.drain_trace();

    assert_eq!(asked, Some(WakeReason::MustMove));
    assert!(moved_twice, "asked again while moving");
    let start = engaged_at(&trace);
    let placed = times(&trace, |e| matches!(e, TraceEvent::Demand { .. }));
    let met = times(&trace, |e| matches!(e, TraceEvent::DemandMet { .. }));
    assert_eq!(placed, vec![start + SimDuration(3000)]);
    assert_eq!(met.len(), 1);
    // 8 yards at 7 yards a second.
    let took = met[0] - placed[0];
    assert!(took.millis().abs_diff(1143) <= 1, "{took:?}");
    assert_eq!(outcome.seats[0].demands_failed, 0);
    assert_eq!(outcome.seats[0].damage_taken, 0);
}

#[test]
fn standing_still_fails_a_demand() {
    let f = with_rules(vec![swirl(vec![flat(5000.0)])]);
    let mut k = kernel(&f, 5, 1, instant());
    let outcome = play(&mut k);
    let trace = k.drain_trace();

    let start = engaged_at(&trace);
    let failed = times(&trace, |e| matches!(e, TraceEvent::DemandFailed { .. }));
    assert_eq!(failed, vec![start + SimDuration(4500)]);
    assert_eq!(outcome.seats[0].demands_failed, 1);
    assert_eq!(outcome.seats[0].damage_taken, 5000);
}

#[test]
fn out_of_range_targets_wait_for_an_approach() {
    let leap = rule(
        "leap",
        at(2000),
        EnemyAction::Reposition {
            distance: Dist::Fixed(50.0),
        },
    );
    let f = with_rules(vec![leap]);
    let mut k = kernel(&f, 6, 1, instant());
    let mut approached: Option<SimTime> = None;
    let mut arrived: Option<SimTime> = None;
    let outcome = drive(&mut k, |k, req| {
        let state = k.state();
        let me = state.seats()[usize::from(req.seat.0)];
        let Some(enemy) = state.target(me) else {
            return priority(k, req.seat);
        };
        let distance = state.distance(req.seat, enemy).expect("an enemy");
        if distance <= 40.0 + 1e-6 {
            if approached.is_some() && arrived.is_none() {
                arrived = Some(req.now);
            }
            return priority(k, req.seat);
        }
        if state.movement(req.seat).is_some() {
            return Choice::Wait(Wait::NextEvent);
        }
        let mask = k.legal(req.seat);
        assert_eq!(mask.abilities[&LIGHTNING_BOLT], Readiness::Blocked);
        assert!(matches!(
            k.clone().submit(req.seat, cast(LIGHTNING_BOLT)),
            Err(EngineError::Illegal {
                reason: IllegalChoice::OutOfRange(_),
                ..
            })
        ));
        approached = Some(req.now);
        Choice::Move(MoveGoal::Approach {
            target: enemy,
            within: 40.0,
        })
    });

    assert!(outcome.completed, "{outcome:?}");
    let (approached, arrived) = (approached.unwrap(), arrived.unwrap());
    // 10 yards at 7 yards a second.
    let took = arrived - approached;
    assert!(took.millis().abs_diff(1429) <= 1, "{took:?}");
}

#[test]
fn seats_recover_between_pulls() {
    let mut f = with_rules(vec![rule(
        "execute",
        at(1000),
        EnemyAction::Damage {
            amount: Dist::Fixed(1e12),
            school: SchoolMask::PHYSICAL,
            target: EnemyTarget::Tank,
        },
    )]);
    let mut spec = f.sampler.spec().clone();
    let mut second = spec.pulls[0].clone();
    second.name = portunus_core::PullName("again".into());
    spec.pulls.push(second);
    f.sampler = Sampler::new(spec, Arc::clone(&f.enemies)).unwrap();
    let mut k = kernel(&f, 7, 2, instant());
    let outcome = play(&mut k);
    let trace = k.drain_trace();

    assert!(outcome.completed, "{outcome:?}");
    // The first seat dies in each pull and is back for the next.
    assert_eq!(outcome.seats[0].deaths, 2);
    assert_eq!(outcome.seats[1].deaths, 0);
    let revived = times(&trace, |e| matches!(e, TraceEvent::Resurrect { .. }));
    assert_eq!(revived.len(), 1);
    let second_pull = trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::CombatStart { .. }))
        .nth(1)
        .expect("a second pull")
        .time;
    assert!(revived[0] < second_pull);
    let me = k.state().seats()[0];
    let casts_after = trace
        .iter()
        .filter(|r| r.time >= second_pull)
        .filter(|r| matches!(r.event, TraceEvent::CastStart { actor, .. } if actor == me))
        .count();
    assert!(casts_after > 0, "the revived seat plays the second pull");
}
