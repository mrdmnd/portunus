//! Empowers: stages, release by option, by `StopCast`, by movement, and
//! when the hold runs out; fizzling before the first stage; haste.

mod common;

use std::sync::Arc;

use portunus_core::{AuraId, Dist, EnemyKey, EventName, SimDuration, SimTime, Trigger};
use portunus_engine::state::SegmentView;
use portunus_engine::trace::{CastEndReason, TraceEvent, TraceRecord};
use portunus_engine::{
    CastOpts, Choice, DecisionRequest, Engine, Kernel, Readiness, StateView, Step, TargetSel, Wait,
    WakeReason,
};
use portunus_gamedata::aura::{AuraDef, RefreshRule};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget, ModKind, ModScope, Modifier};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemySubject, EnemyTarget};
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::stats::SchoolMask;
use portunus_scenario::{Sampler, ScenarioSampler};

use common::*;

const BASE: u64 = 1000;
const HOVER: AuraId = AuraId(900_501);

fn flat(amount: u64) -> Effect {
    Effect::Damage {
        amount: Coefficient::Flat(amount as f64),
        school: SchoolMask::FIRE,
        target: EffectTarget::Target,
        aoe: None,
        ignores_armor: false,
        hand: None,
        per_count: None,
        unmodified: false,
    }
}

/// Lightning Bolt as the seat's only ability, turned into an empower with
/// stages at 1, 1.75, and 2.5 s and a 2 s hold. It deals [`BASE`], then
/// 100 times the stage reached.
fn empower_fixture(haste_pct: f64, rules: Vec<EnemyRule>) -> Fixture {
    let mut f = fixture();
    let data = Arc::make_mut(&mut f.data);
    let bolt = data
        .spells
        .get_mut(&LIGHTNING_BOLT)
        .expect("Lightning Bolt");
    bolt.cast = CastKind::Empower {
        stages: [1000, 1750, 2500].map(SimDuration).to_vec(),
        hasted: true,
        hold: SimDuration(2000),
        stage_effects: (1..=3).map(|n| vec![flat(100 * n)]).collect(),
    };
    bolt.speed = None;
    bolt.effects = vec![flat(BASE)];
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

fn empower(to: Option<u8>, stage_wakes: bool) -> Choice {
    Choice::Cast {
        ability: LIGHTNING_BOLT,
        target: TargetSel::Primary,
        opts: CastOpts {
            empower: to,
            tick_wakes: stage_wakes,
        },
    }
}

/// Run for 20 s of combat. `answer` gets first say; otherwise empower to
/// `to` whenever Lightning Bolt is ready.
fn drive(
    k: &mut Kernel<Stub>,
    to: Option<u8>,
    mut answer: impl FnMut(&Kernel<Stub>, &DecisionRequest) -> Option<Choice>,
) -> Vec<TraceRecord> {
    let mut start = None;
    for _ in 0..100_000 {
        match k.advance().unwrap() {
            Step::Done(_) => break,
            Step::Decide(req) => {
                if matches!(k.state().segment(), SegmentView::Combat(_)) {
                    let t0 = *start.get_or_insert(req.now);
                    if req.now > t0 + SimDuration(20_000) {
                        break;
                    }
                }
                let choice = answer(k, &req).unwrap_or_else(|| {
                    if k.legal(req.seat).abilities[&LIGHTNING_BOLT] == Readiness::Now {
                        empower(to, false)
                    } else {
                        Choice::Wait(Wait::NextEvent)
                    }
                });
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    k.drain_trace()
}

fn starts(trace: &[TraceRecord]) -> Vec<SimTime> {
    trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::CastStart { .. }))
        .map(|r| r.time)
        .collect()
}

fn stages(trace: &[TraceRecord], until: SimTime) -> Vec<(SimTime, u8)> {
    trace
        .iter()
        .filter(|r| r.time <= until)
        .filter_map(|r| match r.event {
            TraceEvent::EmpowerStage { stage, .. } => Some((r.time, stage)),
            _ => None,
        })
        .collect()
}

fn first_end(trace: &[TraceRecord]) -> (SimTime, CastEndReason) {
    trace
        .iter()
        .find_map(|r| match r.event {
            TraceEvent::CastEnd { reason, .. } => Some((r.time, reason)),
            _ => None,
        })
        .expect("a cast ends")
}

/// Damage dealt up to `until`, as `(time, amount)`.
fn hits(trace: &[TraceRecord], until: SimTime) -> Vec<(SimTime, u64)> {
    trace
        .iter()
        .filter(|r| r.time <= until)
        .filter_map(|r| match &r.event {
            TraceEvent::Damage(d) => Some((r.time, d.amount)),
            _ => None,
        })
        .collect()
}

fn ms(t: SimTime, ms: u32) -> SimTime {
    t + SimDuration(ms)
}

#[test]
fn an_empower_releases_at_the_stage_it_was_cast_to() {
    let f = empower_fixture(0.0, Vec::new());
    let mut k = kernel(&f, 1, 1, instant());
    let trace = drive(&mut k, Some(2), |_, _| None);

    let t0 = starts(&trace)[0];
    let released = ms(t0, 1750);
    assert_eq!(
        stages(&trace, released),
        vec![(ms(t0, 1000), 1), (released, 2)]
    );
    assert_eq!(first_end(&trace), (released, CastEndReason::Completed));
    assert_eq!(
        hits(&trace, released),
        vec![(released, BASE), (released, 200)]
    );
    // The GCD starts again on release.
    assert_eq!(starts(&trace)[1], ms(released, 1500));
}

#[test]
fn an_empower_held_too_long_releases_at_its_final_stage() {
    let f = empower_fixture(0.0, Vec::new());
    let mut k = kernel(&f, 1, 1, instant());
    let trace = drive(&mut k, None, |_, _| None);

    let t0 = starts(&trace)[0];
    let released = ms(t0, 4500);
    assert_eq!(
        stages(&trace, released),
        vec![(ms(t0, 1000), 1), (ms(t0, 1750), 2), (ms(t0, 2500), 3)]
    );
    assert_eq!(first_end(&trace), (released, CastEndReason::Completed));
    assert_eq!(
        hits(&trace, released),
        vec![(released, BASE), (released, 300)]
    );
}

#[test]
fn haste_shortens_stages_and_the_hold() {
    let f = empower_fixture(50.0, Vec::new());
    let mut k = kernel(&f, 1, 1, instant());
    let trace = drive(&mut k, None, |_, _| None);

    let t0 = starts(&trace)[0];
    assert_eq!(
        stages(&trace, ms(t0, 3000)),
        vec![(ms(t0, 667), 1), (ms(t0, 1167), 2), (ms(t0, 1667), 3)]
    );
    assert_eq!(first_end(&trace), (ms(t0, 3000), CastEndReason::Completed));
}

#[test]
fn stopping_an_empower_releases_the_stage_reached() {
    let f = empower_fixture(0.0, Vec::new());
    let mut k = kernel(&f, 1, 1, instant());
    let mut seen = Vec::new();
    let mut first = true;
    let trace = drive(&mut k, None, |k, req| {
        let me = k.state().seats()[usize::from(req.seat.0)];
        if let Some(c) = k.state().actor(me).and_then(|a| a.casting) {
            seen.push((req.now, req.reason, c.empower_stage, c.next_stage_at));
            first = false;
            return Some(Choice::StopCast);
        }
        first.then(|| empower(None, true))
    });

    let t0 = starts(&trace)[0];
    let released = ms(t0, 1000);
    assert_eq!(
        seen,
        vec![(
            released,
            WakeReason::EmpowerStage(1),
            Some(1),
            Some(ms(t0, 1750))
        )]
    );
    assert_eq!(first_end(&trace), (released, CastEndReason::Completed));
    assert_eq!(
        hits(&trace, released),
        vec![(released, BASE), (released, 100)]
    );
    assert_eq!(starts(&trace)[1], ms(released, 1500));
}

#[test]
fn an_empower_stopped_before_its_first_stage_fizzles() {
    let roar = enemy_rule(
        "roar",
        500,
        EnemyAction::Cast {
            time: SimDuration(4000),
            interruptible: false,
            then: Box::new(EnemyAction::Effects {
                effects: Vec::new(),
                target: EnemyTarget::AllPlayers,
            }),
        },
    );
    let f = empower_fixture(0.0, vec![roar]);
    let mut k = kernel(&f, 1, 1, instant());
    let trace = drive(&mut k, None, |k, req| {
        let me = k.state().seats()[usize::from(req.seat.0)];
        let charging = k.state().actor(me).and_then(|a| a.casting).is_some();
        (charging && matches!(req.reason, WakeReason::EnemyCastStart(_)))
            .then_some(Choice::StopCast)
    });

    let t0 = starts(&trace)[0];
    assert_eq!(first_end(&trace), (ms(t0, 500), CastEndReason::Stopped));
    assert!(stages(&trace, ms(t0, 1500)).is_empty());
    assert!(hits(&trace, ms(t0, 1499)).is_empty());
    // No release, so no fresh GCD: the next cast waits only for the first.
    assert_eq!(starts(&trace)[1], ms(t0, 1500));
}

fn push_at(at_ms: u32) -> EnemyRule {
    enemy_rule(
        "push",
        at_ms,
        EnemyAction::ForceMovement {
            duration: Dist::Fixed(SimDuration(500)),
            target: EnemyTarget::AllPlayers,
        },
    )
}

#[test]
fn moving_releases_an_empower_unless_it_can_be_cast_while_moving() {
    let f = empower_fixture(0.0, vec![push_at(1500)]);
    let mut k = kernel(&f, 1, 1, instant());
    let trace = drive(&mut k, None, |_, _| None);
    let t0 = starts(&trace)[0];
    let pushed = ms(t0, 1500);
    assert_eq!(first_end(&trace), (pushed, CastEndReason::Completed));
    assert_eq!(hits(&trace, pushed), vec![(pushed, BASE), (pushed, 100)]);

    let mut f = empower_fixture(0.0, vec![push_at(1500)]);
    Arc::make_mut(&mut f.data).auras.insert(
        HOVER,
        AuraDef {
            id: HOVER,
            name: "hover".into(),
            duration: None,
            max_stacks: 1,
            refresh: RefreshRule::Replace,
            periodic: None,
            value: None,
            modifiers: vec![Modifier {
                scope: ModScope::Spell(LIGHTNING_BOLT),
                kind: ModKind::CastWhileMoving,
                value: 1.0,
                per_stack: false,
                condition: None,
            }],
            listeners: Vec::new(),
            overrides: Vec::new(),
            on_expire: Vec::new(),
            cancelable: false,
            blocked_by: None,
            form: None,
            stealth: None,
            ends_with: None,
            persists_through_death: false,
            unique_per_source: false,
            prevents_death: None,
            ground: None,
        },
    );
    f.template.passive_auras.push(HOVER);
    let mut k = kernel(&f, 1, 1, instant());
    let trace = drive(&mut k, None, |_, _| None);
    let t0 = starts(&trace)[0];
    assert_eq!(first_end(&trace), (ms(t0, 4500), CastEndReason::Completed));
}
