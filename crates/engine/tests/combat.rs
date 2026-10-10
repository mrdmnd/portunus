mod common;

use std::sync::Arc;

use portunus_core::{
    ActorId, AuraId, Dist, EnemyKey, EventName, PullName, SimDuration, SimTime, Trigger,
};
use portunus_engine::trace::{CastEndReason, TraceEvent, TraceRecord};
use portunus_engine::{
    Choice, DecisionRequest, Engine, Externals, Kernel, Latency, MoveGoal, Outcome, PullAura,
    StateView, Step, Wait, WakeReason,
};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemySubject};
use portunus_gamedata::item::WeaponDef;
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::SchoolMask;
use portunus_scenario::{Sampler, ScenarioSampler};

use common::*;

const SPEED: u32 = 2600;
const BLOODLUST: AuraId = AuraId(2825);
const SATED: AuraId = AuraId(57724);

/// The fixture with a melee seat swinging a `SPEED` weapon at a dummy of
/// exactly `health` running `rules`.
fn melee(health: u64, rules: Vec<EnemyRule>) -> Fixture {
    let mut f = with_rules(rules);
    f.template.melee = true;
    f.template.main_hand = Some(WeaponDef {
        speed: SimDuration(SPEED),
        min_damage: 100.0,
        max_damage: 100.0,
        ranged: false,
    });
    let mut enemies = (*f.enemies).clone();
    enemies
        .enemies
        .get_mut(&EnemyKey("target_dummy".into()))
        .expect("the dummy exists")
        .health = health;
    f.enemies = Arc::new(enemies);
    let mut spec = f.sampler.spec().clone();
    spec.pulls[0].health = Dist::Fixed(1.0);
    f.sampler = Sampler::new(spec, Arc::clone(&f.enemies)).unwrap();
    f
}

/// The fixture with the dummy running `rules`.
fn with_rules(rules: Vec<EnemyRule>) -> Fixture {
    let mut f = fixture();
    let mut enemies = (*f.enemies).clone();
    enemies
        .enemies
        .get_mut(&EnemyKey("target_dummy".into()))
        .expect("the dummy exists")
        .rules = rules;
    f.enemies = Arc::new(enemies);
    f.sampler = Sampler::new(f.sampler.spec().clone(), Arc::clone(&f.enemies)).unwrap();
    f
}

fn leap(name: &str, at: u32, distance: f64) -> EnemyRule {
    leap_every(name, at, distance, None)
}

fn leap_every(name: &str, at: u32, distance: f64, every: Option<u32>) -> EnemyRule {
    EnemyRule {
        name: EventName(name.into()),
        phase: None,
        when: Trigger::<EnemySubject>::Elapsed(SimDuration(at)),
        repeat: every.map(|ms| Dist::Fixed(SimDuration(ms))),
        action: EnemyAction::Reposition {
            distance: Dist::Fixed(distance),
        },
    }
}

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

fn times(trace: &[TraceRecord], pick: impl Fn(&TraceEvent) -> bool) -> Vec<SimTime> {
    trace
        .iter()
        .filter(|r| pick(&r.event))
        .map(|r| r.time)
        .collect()
}

/// The stub's swings: one point of physical damage with no spell.
fn swings(trace: &[TraceRecord], k: &Kernel<Stub>) -> Vec<SimTime> {
    let me = k.state().seats()[0];
    times(trace, |e| {
        matches!(e, TraceEvent::Damage(d)
            if d.source == me && d.spell.is_none() && d.school == SchoolMask::PHYSICAL)
    })
}

#[test]
fn melee_swings_follow_the_target_in_and_out_of_reach() {
    let f = melee(30, vec![leap("leap", 20_000, 30.0)]);
    let mut k = kernel(&f, 1, 1, instant());
    let outcome = drive(&mut k, |k, req| {
        let state = k.state();
        let me = state.seats()[0];
        match state.target(me) {
            Some(enemy)
                if state.movement(req.seat).is_none()
                    && state.distance(req.seat, enemy).unwrap() > MELEE_RANGE + 1e-6 =>
            {
                Choice::Move(MoveGoal::Approach {
                    target: enemy,
                    within: MELEE_RANGE,
                })
            }
            _ => Choice::Wait(Wait::NextEvent),
        }
    });
    let trace = k.drain_trace();
    assert!(outcome.completed, "{outcome:?}");

    let engaged = times(&trace, |e| matches!(e, TraceEvent::Engage { .. }))[0];
    let leapt = engaged + SimDuration(20_000);
    let arrived = times(&trace, |e| matches!(e, TraceEvent::MovementEnd { .. }))[0];
    let hits = swings(&trace, &k);
    assert_eq!(hits.len(), 30);
    // Melee seats open the pull in reach and swing at once.
    assert_eq!(hits[0], engaged);
    let before: Vec<_> = hits.iter().copied().filter(|&t| t <= leapt).collect();
    for w in before.windows(2) {
        assert_eq!((w[1] - w[0]).millis(), SPEED);
    }
    assert!(hits.iter().all(|&t| t <= leapt || t >= arrived));
    let resumed = hits.iter().copied().find(|&t| t > leapt).unwrap();
    assert_eq!(resumed, arrived, "the swing held while out of reach");
}

#[test]
fn hard_casts_hold_swings_until_they_end() {
    let f = melee(1_000_000_000, vec![]);
    let mut k = kernel(&f, 2, 1, instant());
    play_for(&mut k, 200);
    let trace = k.drain_trace();
    let me = k.state().seats()[0];

    let mut casts = Vec::new();
    let mut started = None;
    for r in &trace {
        match r.event {
            TraceEvent::CastStart { actor, .. } if actor == me => started = Some(r.time),
            TraceEvent::CastEnd {
                actor,
                reason: CastEndReason::Completed,
                ..
            } if actor == me => {
                let start = started.take().unwrap();
                if r.time > start {
                    casts.push((start, r.time));
                }
            }
            _ => {}
        }
    }
    assert!(casts.len() > 10, "{} hard casts", casts.len());
    let hits = swings(&trace, &k);
    assert!(hits.len() > 10, "{} swings", hits.len());
    for &t in &hits {
        assert!(
            !casts.iter().any(|&(s, e)| s < t && t < e),
            "swing at {t:?} inside a cast"
        );
    }
    let held = hits
        .iter()
        .filter(|&&t| casts.iter().any(|&(_, e)| e == t))
        .count();
    assert!(held > 0, "some swings wait out a cast");
}

fn lagged(lag: u32) -> Latency {
    Latency {
        cast_lag: Dist::Fixed(SimDuration(lag)),
        ..instant()
    }
}

#[test]
fn cast_lag_delays_only_unanticipated_casts() {
    // The dummy shuffling about wakes the seat unprompted.
    let f = with_rules(vec![
        leap_every("back", 3000, 25.0, Some(3000)),
        leap_every("forth", 4500, 20.0, Some(3000)),
    ]);
    let mut k = kernel(&f, 3, 1, lagged(100));
    let mut decided = Vec::new();
    let mut asked = 0;
    drive(&mut k, |k, req| {
        // Idling now and then leaves the dummy's moves to interrupt.
        asked += 1;
        if req.anticipated && asked % 3 == 0 {
            return Choice::Wait(Wait::NextEvent);
        }
        let choice = priority(k, req.seat);
        if matches!(choice, Choice::Cast { .. }) {
            decided.push((req.now, req.anticipated));
        }
        choice
    });
    let trace = k.drain_trace();
    let me = k.state().seats()[0];
    let starts = times(
        &trace,
        |e| matches!(e, TraceEvent::CastStart { actor, .. } if *actor == me),
    );

    let unanticipated = decided.iter().filter(|d| !d.1).count();
    assert!(unanticipated > 0);
    assert_eq!(starts.len(), decided.len());
    for (&(at, anticipated), &start) in decided.iter().zip(&starts) {
        let lag = if anticipated { 0 } else { 100 };
        assert_eq!((start - at).millis(), lag, "decided at {at:?}");
    }
}

#[test]
fn a_lagged_cast_that_became_illegal_asks_again() {
    let f = melee(
        1_000_000_000,
        vec![leap("near", 1000, 20.0), leap("far", 1200, 60.0)],
    );
    let mut k = kernel(&f, 4, 1, lagged(500));
    let mut cast_at = None;
    let mut failed_at = None;
    for _ in 0..50 {
        let Step::Decide(req) = k.advance().unwrap() else {
            break;
        };
        let choice = match req.reason {
            WakeReason::EnemyMoved(_) if cast_at.is_none() => {
                cast_at = Some(req.now);
                cast(LIGHTNING_BOLT)
            }
            WakeReason::CastFailed => {
                failed_at = Some(req.now);
                break;
            }
            _ => Choice::Wait(Wait::NextEvent),
        };
        k.submit(req.seat, choice).unwrap();
    }
    let trace = k.drain_trace();
    let (cast_at, failed_at) = (cast_at.unwrap(), failed_at.expect("the cast failed"));
    assert_eq!((failed_at - cast_at).millis(), 500);
    let me = k.state().seats()[0];
    let starts = times(
        &trace,
        |e| matches!(e, TraceEvent::CastStart { actor, .. } if *actor == me),
    );
    assert!(starts.is_empty(), "{starts:?}");
}

#[test]
fn on_pull_auras_respect_their_lockout_across_pulls() {
    let mut f = fixture();
    let mut spec = f.sampler.spec().clone();
    let mut second = spec.pulls[0].clone();
    second.name = PullName("again".into());
    spec.pulls.push(second);
    f.sampler = Sampler::new(spec, Arc::clone(&f.enemies)).unwrap();
    let mut s = setup(&f, 5, 2, instant());
    s.externals = Externals {
        on_pull: vec![PullAura {
            aura: BLOODLUST,
            lockout: Some(SATED),
        }],
        ..Externals::default()
    };
    let mut k = Kernel::new(s, Stub).unwrap();
    let outcome = play(&mut k);
    let trace = k.drain_trace();
    assert!(outcome.completed, "{outcome:?}");

    let pulls = times(&trace, |e| matches!(e, TraceEvent::CombatStart { .. }));
    assert_eq!(pulls.len(), 2);
    for aura in [BLOODLUST, SATED] {
        let applied = times(
            &trace,
            |e| matches!(e, TraceEvent::AuraApplied { aura: a, .. } if *a == aura),
        );
        // Once per seat, at the first pull only.
        assert_eq!(applied, vec![pulls[0]; 2], "{aura:?}");
    }
}

/// Auras applied to `holder` at run start, before any pull.
fn at_start(trace: &[TraceRecord], holder: ActorId) -> Vec<AuraId> {
    trace
        .iter()
        .filter(|r| r.time == SimTime::ZERO)
        .filter_map(|r| match r.event {
            TraceEvent::AuraApplied {
                holder: h, aura, ..
            } if h == holder => Some(aura),
            _ => None,
        })
        .collect()
}

#[test]
fn raid_buffs_come_from_the_classes_in_the_group() {
    const SKYFURY: AuraId = AuraId(462_854);
    const ARCANE_INTELLECT: AuraId = AuraId(1459);
    const MYSTIC_TOUCH: AuraId = AuraId(113_746);
    let run = |f: &Fixture| {
        let mut k = kernel(f, 6, 2, instant());
        play_for(&mut k, 50);
        let trace = k.drain_trace();
        let state = k.state();
        let enemy = state.enemies()[0];
        let seats: Vec<_> = state.seats().iter().map(|&s| at_start(&trace, s)).collect();
        let debuffs: Vec<_> = trace
            .iter()
            .filter_map(|r| match r.event {
                TraceEvent::AuraApplied { holder, aura, .. } if holder == enemy => Some(aura),
                _ => None,
            })
            .collect();
        let lust = times(
            &trace,
            |e| matches!(e, TraceEvent::AuraApplied { aura, .. } if *aura == BLOODLUST),
        );
        (seats, debuffs, lust.len())
    };

    // Two shamans: Skyfury once on each seat, and one Bloodlust each.
    let f = fixture();
    let (seats, debuffs, lusts) = run(&f);
    for auras in &seats {
        assert_eq!(auras.iter().filter(|&&a| a == SKYFURY).count(), 1);
        assert!(!auras.contains(&ARCANE_INTELLECT));
    }
    assert!(!debuffs.contains(&MYSTIC_TOUCH));
    assert_eq!(lusts, 2);

    // The same spec relabelled a monk brings Mystic Touch and nothing else.
    let mut f = fixture();
    let mut data = (*f.data).clone();
    data.specs.get_mut(&f.template.spec).unwrap().class = "Monk".into();
    f.data = Arc::new(data);
    let (seats, debuffs, lusts) = run(&f);
    assert!(seats.iter().all(|a| !a.contains(&SKYFURY)));
    assert!(debuffs.contains(&MYSTIC_TOUCH));
    assert_eq!(lusts, 0);
}
