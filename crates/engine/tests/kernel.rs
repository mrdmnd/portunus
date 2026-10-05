mod common;

use portunus_core::{SimDuration, SimTime};
use portunus_engine::choice::{CmpOp, Condition, Scalar};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    Choice, Engine, EngineError, IllegalChoice, Kernel, Readiness, SetupIssue, StateView, Step,
    Wait, WakeReason,
};
use portunus_scenario::resolved::Segment;

use common::*;

fn next_request(k: &mut Kernel<Stub>) -> portunus_engine::DecisionRequest {
    match k.advance().unwrap() {
        Step::Decide(req) => req,
        Step::Done(o) => panic!("run ended early: {o:?}"),
    }
}

#[test]
fn rejects_bad_setups() {
    let f = fixture();
    let mut s = setup(&f, 1, 1, human());
    s.seats.clear();
    let Err(EngineError::Setup(issues)) = Kernel::new(s, Stub) else {
        panic!("expected a setup error");
    };
    assert_eq!(issues, vec![SetupIssue::NoSeats]);

    let mut s = setup(&f, 1, 1, human());
    s.seats[0]
        .template
        .abilities
        .insert(portunus_core::SpellId(1));
    let Err(EngineError::Setup(issues)) = Kernel::new(s, Stub) else {
        panic!("expected a setup error");
    };
    assert_eq!(
        issues,
        vec![SetupIssue::UnknownSpell(portunus_core::SpellId(1))]
    );
}

#[test]
fn dummy_dies_and_gates_hold() {
    let f = fixture();
    let s = setup(&f, 3, 1, human());
    let Segment::Combat(combat) = &s.run.segments[1] else {
        panic!("expected travel then combat");
    };
    let health = combat.spawns[0].max_health;
    let mut k = Kernel::new(s, Stub).unwrap();
    let outcome = play(&mut k);

    assert!(outcome.completed, "{outcome:?}");
    assert!(outcome.pulls[0].cleared.is_some());
    let done = outcome.seats[0].damage_done;
    assert!((done - health).abs() < 1e-6 * health, "{done} vs {health}");

    let trace = k.drain_trace();
    let starts: Vec<(SimTime, portunus_core::SpellId)> = trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::CastStart { spell, .. } => Some((r.time, spell)),
            _ => None,
        })
        .collect();
    assert!(starts.len() > 50);
    for pair in starts.windows(2) {
        let gap = pair[1].0 - pair[0].0;
        assert!(gap >= SimDuration(1500), "GCD violated: {pair:?}");
    }
    let lava: Vec<SimTime> = starts
        .iter()
        .filter(|(_, s)| *s == LAVA_BURST)
        .map(|(t, _)| *t)
        .collect();
    assert!(lava.len() > 5);
    for pair in lava.windows(2) {
        // 2 s cast, then an 8 s cooldown from completion.
        assert!(pair[1] - pair[0] >= SimDuration(10_000), "{pair:?}");
    }
    let ticks = trace
        .iter()
        .filter(|r| matches!(&r.event, TraceEvent::Damage(d) if d.spell.is_none()))
        .count();
    assert!(ticks > 20, "Flame Shock should tick: {ticks}");
}

#[test]
fn same_seed_same_rollout() {
    let f = fixture();
    let mut a = kernel(&f, 11, 1, human());
    let mut b = kernel(&f, 11, 1, human());
    let mut c = kernel(&f, 12, 1, human());
    let (oa, ob, oc) = (play(&mut a), play(&mut b), play(&mut c));
    assert_eq!(oa, ob);
    assert_eq!(a.trace_hash(), b.trace_hash());
    assert_ne!(a.trace_hash(), c.trace_hash());
    assert_ne!(oa.end_time, oc.end_time);
}

#[test]
fn fork_matches_replay() {
    let f = fixture();
    let mut k = kernel(&f, 5, 1, human());
    assert!(play_for(&mut k, 40).is_none());
    let mut fork = k.clone();
    let original = play(&mut k);
    let forked = play(&mut fork);
    assert_eq!(original, forked);
    assert_eq!(k.trace_hash(), fork.trace_hash());
}

#[test]
fn batches_share_one_snapshot() {
    let f = fixture();
    let mut k = kernel(&f, 2, 2, instant());
    let first = next_request(&mut k);
    assert_eq!(first.seat.0, 0);
    assert_eq!(first.reason, WakeReason::CombatStart);
    k.submit(first.seat, cast(LIGHTNING_BOLT)).unwrap();

    let second = next_request(&mut k);
    assert_eq!(second.seat.0, 1);
    assert_eq!(second.now, first.now);
    let seat0 = k.state().seats()[0];
    assert!(
        k.state().actor(seat0).unwrap().casting.is_none(),
        "seat 1 must not see seat 0's same-moment choice"
    );
    k.submit(second.seat, cast(LIGHTNING_BOLT)).unwrap();
    assert!(k.state().actor(seat0).unwrap().casting.is_some());
}

#[test]
fn wait_until_wakes_on_time_and_repeats_are_livelocks() {
    let f = fixture();
    let mut k = kernel(&f, 4, 1, instant());
    let start = next_request(&mut k);
    let later = start.now + SimDuration(5000);
    k.submit(start.seat, Choice::Wait(Wait::Until(later)))
        .unwrap();

    let woke = next_request(&mut k);
    assert_eq!(woke.now, later);
    assert_eq!(woke.reason, WakeReason::WaitElapsed);
    assert!(woke.anticipated);

    k.submit(woke.seat, Choice::Wait(Wait::Until(later)))
        .unwrap();
    let again = next_request(&mut k);
    assert_eq!(again.now, later);
    let err = k.submit(again.seat, Choice::Wait(Wait::Until(later)));
    assert!(matches!(err, Err(EngineError::Livelock(_))), "{err:?}");
}

#[test]
fn condition_wait_wakes_when_the_cooldown_returns() {
    let f = fixture();
    let mut k = kernel(&f, 6, 1, instant());
    let start = next_request(&mut k);
    k.submit(start.seat, cast(LAVA_BURST)).unwrap();

    let cast_end = next_request(&mut k);
    assert_eq!(cast_end.reason, WakeReason::CastEnd);
    assert_eq!(cast_end.now, start.now + SimDuration(2000));
    let mask = k.legal(cast_end.seat);
    assert_eq!(
        mask.abilities[&LAVA_BURST],
        Readiness::In(SimDuration(8000))
    );
    let err = k.submit(cast_end.seat, cast(LAVA_BURST));
    assert!(matches!(
        err,
        Err(EngineError::Illegal {
            reason: IllegalChoice::NotReady { .. },
            ..
        })
    ));

    let ready = Condition::Cmp {
        lhs: Scalar::CooldownRemaining(LAVA_BURST),
        op: CmpOp::Le,
        rhs: 0.0,
    };
    k.submit(cast_end.seat, Choice::Wait(Wait::Condition(ready)))
        .unwrap();
    let woke = next_request(&mut k);
    assert_eq!(woke.reason, WakeReason::ConditionMet);
    assert_eq!(woke.now, cast_end.now + SimDuration(8000));
    assert_eq!(k.legal(woke.seat).abilities[&LAVA_BURST], Readiness::Now);
}

#[test]
fn answering_out_of_turn_is_rejected() {
    let f = fixture();
    let mut k = kernel(&f, 8, 2, instant());
    let first = next_request(&mut k);
    let other = portunus_core::Seat(1 - first.seat.0);
    assert!(matches!(
        k.submit(other, Choice::Wait(Wait::NextEvent)),
        Err(EngineError::WrongSeat(_))
    ));
}
