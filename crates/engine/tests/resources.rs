//! Resources that regenerate linearly: running dry and waiting for regen,
//! and decay (or a different rate) outside combat.

mod common;

use std::sync::Arc;

use common::*;
use portunus_core::{Dist, PullName, Seat, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{Choice, Engine, Readiness, StateView, Step, Wait};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget};
use portunus_gamedata::spell::{CastKind, SpellDef, Targeting};
use portunus_gamedata::stats::{
    Cost, ResourceAmount, ResourceDef, ResourceKind, SchoolMask, SpendScaling,
};
use portunus_scenario::{Sampler, ScenarioSampler};

const TRAVEL: SimDuration = SimDuration(20_000);
const SHOUT: SpellId = SpellId(900_951);
const SLAM: SpellId = SpellId(900_952);

/// `(time, rage, regen per second)` at a decision.
type Sample = (SimTime, f64, f64);

/// An instant, off-GCD, free spell.
fn free(id: SpellId, hostile: bool, effects: Vec<Effect>) -> SpellDef {
    let base = fixture().data.spells[&EARTH_SHOCK].clone();
    SpellDef {
        id,
        name: format!("spell {}", id.0),
        cast: CastKind::Instant,
        gcd: None,
        cooldown: None,
        costs: Vec::new(),
        targeting: if hostile {
            Targeting::Enemy
        } else {
            Targeting::SelfOnly
        },
        hostile,
        speed: None,
        requires: Vec::new(),
        effects,
        ..base
    }
}

/// Rage from 30: +1 a second in combat, -2 out of it. Two pulls, each
/// after a 20 s walk; Slam kills the dummy and gives 40 rage, and a free
/// self-cast keeps the seat awake through the walks. Samples
/// `(time, rage, regen per second)` once a second, Slamming when it can,
/// and returns the pulls' `(start, end)`.
fn rage_samples() -> (Vec<Sample>, Vec<(SimTime, SimTime)>) {
    let mut f = fixture();
    f.template.resources = vec![ResourceDef {
        kind: ResourceKind::Rage,
        max: 100.0,
        initial: 30.0,
        regen_per_sec: 1.0,
        regen_hasted: false,
        recharge: None,
        out_of_combat: Some(-2.0),
    }];
    let slam = vec![
        Effect::Damage {
            amount: Coefficient::Flat(1e12),
            school: SchoolMask::PHYSICAL,
            target: EffectTarget::Target,
            aoe: None,
            ignores_armor: true,
            hand: None,
            per_count: None,
            unmodified: false,
        },
        Effect::Resource(ResourceAmount {
            kind: ResourceKind::Rage,
            amount: 40.0,
        }),
    ];
    let data = Arc::make_mut(&mut f.data);
    for s in [free(SHOUT, false, Vec::new()), free(SLAM, true, slam)] {
        f.template.abilities.insert(s.id);
        data.spells.insert(s.id, s);
    }
    let mut spec = f.sampler.spec().clone();
    spec.pulls[0].travel_in = Dist::Fixed(TRAVEL);
    spec.pulls[0].prepull = TRAVEL;
    let mut second = spec.pulls[0].clone();
    second.name = PullName("again".into());
    spec.pulls.push(second);
    f.sampler = Sampler::new(spec, Arc::clone(&f.enemies)).unwrap();
    let mut k = kernel(&f, 1, 1, instant());
    let mut samples = Vec::new();
    while let Step::Decide(req) = k.advance().unwrap() {
        let now = req.now;
        let me = k.state().seats()[0];
        let rage = k.state().resource(me, ResourceKind::Rage).unwrap();
        samples.push((now, rage.value, rage.regen_per_sec));
        let choice = if k.legal(Seat(0)).abilities[&SLAM] == Readiness::Now {
            cast(SLAM)
        } else {
            let next = SimTime::ZERO + SimDuration((now.millis() / 1000 + 1) * 1000);
            Choice::Wait(Wait::Until(next))
        };
        k.submit(Seat(0), choice).unwrap();
    }
    let trace = k.drain_trace();
    let at = |starts: bool| {
        trace.iter().filter_map(move |r| match r.event {
            TraceEvent::CombatStart { .. } if starts => Some(r.time),
            TraceEvent::CombatEnd { .. } if !starts => Some(r.time),
            _ => None,
        })
    };
    (samples, at(true).zip(at(false)).collect())
}

#[test]
fn rage_drains_outside_combat_and_stops_at_zero() {
    let (samples, pulls) = rage_samples();
    let secs = |t: SimTime| f64::from(t.millis()) / 1000.0;
    let [(start0, end0), (start1, _)] = pulls[..] else {
        panic!("two pulls: {pulls:?}");
    };
    assert_eq!(start0, SimTime::ZERO + TRAVEL);
    assert_eq!(end0, start0, "Slam kills at once");
    assert_eq!(start1, end0 + TRAVEL);
    let pulling = samples.iter().find(|&&(t, ..)| t == start0);
    assert_eq!(
        pulling.map(|s| s.2),
        Some(1.0),
        "in-combat regen at the pull"
    );
    let mut walked = [0.0_f64; 2];
    for &(t, rage, rate) in &samples {
        let (walk, from, start_rage) = if t < start0 {
            (0, SimTime::ZERO, 30.0)
        } else if t > end0 && t < start1 {
            (1, end0, 40.0)
        } else {
            continue;
        };
        let s = secs(t) - secs(from);
        assert_eq!(rage, (start_rage - 2.0 * s).max(0.0), "at {t:?}");
        assert_eq!(rate, -2.0);
        walked[walk] = walked[walk].max(s);
    }
    assert!(walked.iter().all(|&w| w > 15.0), "drained to 0: {walked:?}");
}

// Mana capped at 300, refilling 50 a second; Drain costs 100 and does
/// nothing.
#[test]
fn a_seat_out_of_mana_waits_for_regen() {
    const DRAIN: SpellId = SpellId(900_953);
    let mut f = fixture();
    f.template.resources = vec![ResourceDef {
        kind: ResourceKind::Mana,
        max: 300.0,
        initial: 300.0,
        regen_per_sec: 50.0,
        regen_hasted: false,
        recharge: None,
        out_of_combat: None,
    }];
    let mut drain = free(DRAIN, true, Vec::new());
    drain.costs = vec![Cost {
        kind: ResourceKind::Mana,
        amount: 100.0,
        extra: 0.0,
        scaling: SpendScaling::None,
    }];
    f.template.abilities.insert(DRAIN);
    Arc::make_mut(&mut f.data).spells.insert(DRAIN, drain);
    let mut k = kernel(&f, 1, 1, instant());
    let mut casts: Vec<SimTime> = Vec::new();
    let mut waits = Vec::new();
    while casts.len() < 6 {
        let Step::Decide(req) = k.advance().unwrap() else {
            panic!("the run ended");
        };
        let me = k.state().seats()[0];
        let choice = match k.legal(Seat(0)).abilities[&DRAIN] {
            _ if k.state().target(me).is_none() => Choice::Wait(Wait::NextEvent),
            Readiness::Now => {
                casts.push(req.now);
                cast(DRAIN)
            }
            Readiness::In(d) => {
                waits.push(d);
                Choice::Wait(Wait::Until(req.now + d))
            }
            Readiness::Blocked => panic!("blocked at {:?}", req.now),
        };
        k.submit(Seat(0), choice).unwrap();
    }
    let t0 = casts[0];
    let regen = SimDuration(2000);
    assert_eq!(
        casts,
        vec![
            t0,
            t0,
            t0,
            t0 + regen,
            t0 + regen + regen,
            t0 + SimDuration(6000)
        ],
        "300 mana is three casts; then one every 100 / 50 s"
    );
    assert!(waits.iter().all(|&d| d == regen), "{waits:?}");
}
