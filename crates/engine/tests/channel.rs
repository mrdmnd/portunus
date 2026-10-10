//! Channels: timing fixed at the start, ticks, stopping, clipping, waking,
//! and forced movement.

mod common;

use std::sync::Arc;

use portunus_core::{Dist, EnemyKey, EventName, SimDuration, SimTime, Trigger};
use portunus_engine::state::ChannelProgress;
use portunus_engine::trace::{CastEndReason, TraceEvent, TraceRecord};
use portunus_engine::{
    CastOpts, Choice, DecisionRequest, Engine, EngineError, IllegalChoice, Kernel, Outcome,
    Readiness, StateView, Step, TargetSel, Wait, WakeReason,
};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemySubject, EnemyTarget};
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::stats::SchoolMask;
use portunus_scenario::{Sampler, ScenarioSampler};

use common::*;

const PER_TICK: u64 = 1000;

/// Lightning Bolt as the seat's only ability, turned into a 3 s channel
/// of three ticks that each deal [`PER_TICK`].
fn channel_fixture(haste_pct: f64, rules: Vec<EnemyRule>) -> Fixture {
    let mut f = fixture();
    let data = Arc::make_mut(&mut f.data);
    let bolt = data
        .spells
        .get_mut(&LIGHTNING_BOLT)
        .expect("Lightning Bolt");
    bolt.cast = CastKind::Channel {
        duration: SimDuration(3000),
        ticks: 3,
        hasted: true,
        swings: false,
    };
    bolt.speed = None;
    bolt.effects = vec![Effect::Damage {
        amount: Coefficient::Flat(PER_TICK as f64),
        school: SchoolMask::NATURE,
        target: EffectTarget::Target,
        aoe: None,
        ignores_armor: false,
        hand: None,
    }];
    f.template.abilities = [LIGHTNING_BOLT].into();
    f.template.derived.haste_pct = haste_pct;
    if !rules.is_empty() {
        let mut enemies = (*f.enemies).clone();
        enemies
            .enemies
            .get_mut(&EnemyKey("target_dummy".into()))
            .expect("the dummy exists")
            .rules = rules;
        f.enemies = Arc::new(enemies);
        f.sampler = Sampler::new(f.sampler.spec().clone(), Arc::clone(&f.enemies)).unwrap();
    }
    f
}

fn enemy_rule(name: &str, at_ms: u32, action: EnemyAction) -> EnemyRule {
    EnemyRule {
        name: EventName(name.into()),
        phase: None,
        when: Trigger::<EnemySubject>::Elapsed(SimDuration(at_ms)),
        repeat: None,
        action,
    }
}

fn channel(tick_wakes: bool) -> Choice {
    Choice::Cast {
        ability: LIGHTNING_BOLT,
        target: TargetSel::Primary,
        opts: CastOpts {
            empower: None,
            tick_wakes,
        },
    }
}

/// Channel whenever Lightning Bolt is ready; `answer` gets first say.
fn drive(
    k: &mut Kernel<Stub>,
    mut answer: impl FnMut(&Kernel<Stub>, &DecisionRequest) -> Option<Choice>,
) -> Outcome {
    for _ in 0..100_000 {
        match k.advance().unwrap() {
            Step::Done(o) => return o,
            Step::Decide(req) => {
                let choice = answer(k, &req).unwrap_or_else(|| {
                    if k.legal(req.seat).abilities[&LIGHTNING_BOLT] == Readiness::Now {
                        channel(false)
                    } else {
                        Choice::Wait(Wait::NextEvent)
                    }
                });
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    panic!("the run should end");
}

fn first_start(trace: &[TraceRecord]) -> SimTime {
    trace
        .iter()
        .find(|r| matches!(r.event, TraceEvent::CastStart { .. }))
        .expect("a cast")
        .time
}

/// `(time, index)` of each channel tick in `[from, to)`.
fn ticks(trace: &[TraceRecord], from: SimTime, to: SimTime) -> Vec<(SimTime, u8)> {
    trace
        .iter()
        .filter(|r| r.time >= from && r.time < to)
        .filter_map(|r| match r.event {
            TraceEvent::ChannelTick { index, .. } => Some((r.time, index)),
            _ => None,
        })
        .collect()
}

fn cast_ends(trace: &[TraceRecord]) -> Vec<(SimTime, CastEndReason)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::CastEnd { reason, .. } => Some((r.time, reason)),
            _ => None,
        })
        .collect()
}

fn ms(t: SimTime, ms: u32) -> SimTime {
    t + SimDuration(ms)
}

#[test]
fn channels_tick_evenly_and_end_with_the_last_tick() {
    let f = channel_fixture(0.0, Vec::new());
    let mut k = kernel(&f, 1, 1, instant());
    let outcome = drive(&mut k, |_, _| None);
    let trace = k.drain_trace();

    let t0 = first_start(&trace);
    assert_eq!(
        ticks(&trace, t0, ms(t0, 3001)),
        vec![(ms(t0, 1000), 1), (ms(t0, 2000), 2), (ms(t0, 3000), 3)]
    );
    assert_eq!(
        cast_ends(&trace)[0],
        (ms(t0, 3000), CastEndReason::Completed)
    );
    // Each tick deals the spell's damage; nothing lands at the start.
    let hits: Vec<SimTime> = trace
        .iter()
        .filter(|r| r.time < ms(t0, 3001))
        .filter_map(|r| match &r.event {
            TraceEvent::Damage(d) if d.amount == PER_TICK => Some(r.time),
            _ => None,
        })
        .collect();
    assert_eq!(hits, vec![ms(t0, 1000), ms(t0, 2000), ms(t0, 3000)]);
    // The next channel starts as the last tick lands.
    let starts: Vec<SimTime> = trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::CastStart { .. }))
        .map(|r| r.time)
        .take(2)
        .collect();
    assert_eq!(starts, vec![t0, ms(t0, 3000)]);
    let tick_count = trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::ChannelTick { .. }))
        .count() as u64;
    assert_eq!(outcome.seats[0].damage_done, PER_TICK * tick_count);
}

#[test]
fn haste_shortens_a_channel_and_its_tick_interval() {
    let f = channel_fixture(50.0, Vec::new());
    let mut k = kernel(&f, 1, 1, instant());
    drive(&mut k, |_, _| None);
    let trace = k.drain_trace();

    let t0 = first_start(&trace);
    assert_eq!(
        ticks(&trace, t0, ms(t0, 2001)),
        vec![(ms(t0, 667), 1), (ms(t0, 1333), 2), (ms(t0, 2000), 3)]
    );
    assert_eq!(
        cast_ends(&trace)[0],
        (ms(t0, 2000), CastEndReason::Completed)
    );
}

#[test]
fn stopping_a_channel_keeps_the_ticks_that_landed() {
    let f = channel_fixture(0.0, Vec::new());
    let mut k = kernel(&f, 1, 1, instant());
    let mut first = true;
    let mut seen = Vec::new();
    drive(&mut k, |k, req| {
        let state = k.state();
        let me = state.seats()[usize::from(req.seat.0)];
        let cast = state.actor(me).and_then(|a| a.casting);
        if let Some(c) = cast {
            assert_eq!(req.reason, WakeReason::ChannelTick);
            assert!(k.legal(req.seat).can_wait_tick);
            seen.push((req.now, c.ticks, c.next_tick));
            // Clip the first channel after its second tick.
            if c.ticks.is_some_and(|p| p.done == 2) {
                first = false;
                return Some(Choice::StopCast);
            }
            return Some(Choice::Wait(Wait::NextEvent));
        }
        first.then(|| channel(true))
    });
    let trace = k.drain_trace();

    let t0 = first_start(&trace);
    let progress = |done| Some(ChannelProgress { done, total: 3 });
    assert_eq!(
        seen,
        vec![
            (ms(t0, 1000), progress(1), Some(ms(t0, 2000))),
            (ms(t0, 2000), progress(2), Some(ms(t0, 3000))),
        ]
    );
    assert_eq!(cast_ends(&trace)[0], (ms(t0, 2000), CastEndReason::Stopped));
    // The stopped channel's third tick never comes; the recast starts its
    // own count at once.
    assert_eq!(
        ticks(&trace, t0, ms(t0, 5001)),
        vec![
            (ms(t0, 1000), 1),
            (ms(t0, 2000), 2),
            (ms(t0, 3000), 1),
            (ms(t0, 4000), 2),
            (ms(t0, 5000), 3),
        ]
    );
}

#[test]
fn waiting_for_a_channel_tick_wakes_right_after_it() {
    let roar = enemy_rule(
        "roar",
        1500,
        EnemyAction::Cast {
            time: SimDuration(4000),
            interruptible: false,
            then: Box::new(EnemyAction::Effects {
                effects: Vec::new(),
                target: EnemyTarget::AllPlayers,
            }),
        },
    );
    let f = channel_fixture(0.0, vec![roar]);
    let mut k = kernel(&f, 1, 1, instant());
    let mut wakes = Vec::new();
    drive(&mut k, |k, req| {
        let me = k.state().seats()[usize::from(req.seat.0)];
        if k.state().actor(me).and_then(|a| a.casting).is_none() {
            assert!(!k.legal(req.seat).can_wait_tick);
            assert!(matches!(
                k.clone().submit(req.seat, Choice::Wait(Wait::ChannelTick)),
                Err(EngineError::Illegal {
                    reason: IllegalChoice::NoChannelTick,
                    ..
                })
            ));
            return None;
        }
        wakes.push((req.now, req.reason));
        Some(if matches!(req.reason, WakeReason::EnemyCastStart(_)) {
            Choice::Wait(Wait::ChannelTick)
        } else {
            Choice::Wait(Wait::NextEvent)
        })
    });
    let trace = k.drain_trace();

    let t0 = first_start(&trace);
    let reasons: Vec<(SimTime, bool)> = wakes
        .iter()
        .map(|&(t, r)| (t, r == WakeReason::ChannelTick))
        .collect();
    // Woken by the enemy's cast mid-channel, then by the next tick only.
    assert_eq!(reasons, vec![(ms(t0, 1500), false), (ms(t0, 2000), true)]);
}

#[test]
fn forced_movement_interrupts_a_channel() {
    let push = enemy_rule(
        "push",
        1500,
        EnemyAction::ForceMovement {
            duration: Dist::Fixed(SimDuration(1000)),
            target: EnemyTarget::AllPlayers,
        },
    );
    let f = channel_fixture(0.0, vec![push]);
    let mut k = kernel(&f, 1, 1, instant());
    drive(&mut k, |_, _| None);
    let trace = k.drain_trace();

    let t0 = first_start(&trace);
    assert_eq!(
        cast_ends(&trace)[0],
        (ms(t0, 1500), CastEndReason::Interrupted)
    );
    assert_eq!(ticks(&trace, t0, ms(t0, 2500)), vec![(ms(t0, 1000), 1)]);
}
