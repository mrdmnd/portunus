//! Spell requirements on the target's health: legality, the target a cast
//! chooses, and the wake when the primary target crosses the threshold.

mod common;

use std::sync::Arc;

use common::*;
use portunus_core::{ActorId, AuraId, Dist, Seat, SimDuration, SpellId};
use portunus_engine::{
    CastOpts, Choice, Engine, EngineError, IllegalChoice, Kernel, Readiness, StateView, Step,
    TargetSel, Wait, WakeReason,
};
use portunus_gamedata::aura::{AuraDef, Periodic, RefreshRule};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget};
use portunus_gamedata::spell::{CastKind, Requirement, SpellDef, Targeting};
use portunus_gamedata::stats::SchoolMask;
use portunus_scenario::{Sampler, ScenarioSampler};

const EXECUTE: SpellId = SpellId(900_801);
const CHIP: SpellId = SpellId(900_802);
const BLEED: SpellId = SpellId(900_803);
const BLEED_DOT: AuraId = AuraId(900_803);
const HEALTH: u64 = 100_000;

fn hit(amount: f64) -> Effect {
    Effect::Damage {
        amount: Coefficient::Flat(amount),
        school: SchoolMask::PHYSICAL,
        target: EffectTarget::Target,
        aoe: None,
        ignores_armor: true,
        hand: None,
        per_count: None,
        unmodified: false,
    }
}

fn spell_with(id: SpellId, effects: Vec<Effect>) -> SpellDef {
    let mut s = base_spell(id);
    s.effects = effects;
    s
}

/// An instant, off-GCD, hostile spell at the enemy with no cost.
fn base_spell(id: SpellId) -> SpellDef {
    let base = fixture().data.spells[&EARTH_SHOCK].clone();
    SpellDef {
        id,
        name: format!("spell {}", id.0),
        cast: CastKind::Instant,
        gcd: None,
        cooldown: None,
        costs: Vec::new(),
        targeting: Targeting::Enemy,
        hostile: true,
        speed: None,
        min_travel: SimDuration::ZERO,
        requires: Vec::new(),
        effects: Vec::new(),
        ..base
    }
}

/// `dummies` dummies of `HEALTH` each; Execute (at most 20%) does nothing,
/// Chip takes 10% a cast, and Bleed takes 5% a second.
fn executioner(dummies: u32) -> Fixture {
    let mut f = fixture();
    let data = Arc::make_mut(&mut f.data);
    let mut execute = spell_with(EXECUTE, Vec::new());
    execute.requires = vec![Requirement::TargetHpAtMost(0.2)];
    let bleed_dot = AuraDef {
        id: BLEED_DOT,
        name: "bleed".into(),
        duration: None,
        max_stacks: 1,
        refresh: RefreshRule::Replace,
        periodic: Some(Periodic {
            period: SimDuration(1000),
            hasted: false,
            partial_final_tick: false,
            effects: vec![hit(HEALTH as f64 / 20.0)],
        }),
        value: None,
        modifiers: Vec::new(),
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
    };
    let apply = Effect::ApplyAura {
        aura: BLEED_DOT,
        target: EffectTarget::Target,
        stacks: 1,
        duration: None,
        per_unit_spent: None,
    };
    for s in [
        execute,
        spell_with(CHIP, vec![hit(HEALTH as f64 / 10.0)]),
        spell_with(BLEED, vec![apply]),
    ] {
        f.template.abilities.insert(s.id);
        data.spells.insert(s.id, s);
    }
    data.auras.insert(BLEED_DOT, bleed_dot);
    for e in Arc::make_mut(&mut f.enemies).enemies.values_mut() {
        e.health = HEALTH;
    }
    let mut spec = f.sampler.spec().clone();
    spec.pulls[0].health = Dist::Fixed(1.0);
    spec.pulls[0].waves[0].mobs[0].1 = dummies;
    f.sampler = Sampler::new(spec, Arc::clone(&f.enemies)).unwrap();
    f
}

fn refusal(k: &mut Kernel<Stub>, choice: Choice) -> IllegalChoice {
    match k.submit(Seat(0), choice) {
        Err(EngineError::Illegal { reason, .. }) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn ready(k: &Kernel<Stub>, ability: SpellId) -> bool {
    k.legal(Seat(0)).abilities.get(&ability) == Some(&Readiness::Now)
}

fn health_frac(k: &Kernel<Stub>, id: ActorId) -> f64 {
    k.state().actor(id).unwrap().health_frac()
}

/// Wait out the pre-pull; the seat's first decision in combat.
fn in_combat(k: &mut Kernel<Stub>) {
    loop {
        let Step::Decide(req) = k.advance().unwrap() else {
            panic!("the run ended before combat");
        };
        if k.state().target(k.state().seats()[0]).is_some() && ready(k, CHIP) {
            return;
        }
        k.submit(req.seat, Choice::Wait(Wait::NextEvent)).unwrap();
    }
}

#[test]
fn execute_is_legal_only_at_or_below_its_threshold() {
    let f = executioner(1);
    let mut k = kernel(&f, 1, 1, instant());
    in_combat(&mut k);
    let me = k.state().seats()[0];
    let target = k.state().target(me).unwrap();
    let mut chips = 0;
    while health_frac(&k, target) > 0.0 {
        let frac = health_frac(&k, target);
        assert_eq!(ready(&k, EXECUTE), frac <= 0.2, "at {frac}");
        if !ready(&k, EXECUTE) {
            let refused = refusal(&mut k, cast(EXECUTE));
            assert!(
                matches!(refused, IllegalChoice::NotReady { .. }),
                "{refused:?}"
            );
        }
        if frac <= 0.2 {
            assert_eq!(chips, 8, "exactly at 20% counts");
            return;
        }
        k.submit(Seat(0), cast(CHIP)).unwrap();
        chips += 1;
        let Step::Decide(_) = k.advance().unwrap() else {
            panic!("the run ended");
        };
    }
    panic!("never reached 20%");
}

#[test]
fn a_stack_requirement_waits_for_enough_stacks() {
    const STACKED: SpellId = SpellId(900_804);
    const STACK_UP: SpellId = SpellId(900_805);
    let mut f = executioner(1);
    let data = Arc::make_mut(&mut f.data);
    let mut buff = data.auras[&BLEED_DOT].clone();
    buff.periodic = None;
    buff.max_stacks = 3;
    data.auras.insert(BLEED_DOT, buff);
    let mut stacked = spell_with(STACKED, Vec::new());
    stacked.requires = vec![Requirement::CasterStacksAtLeast {
        aura: BLEED_DOT,
        stacks: 2,
    }];
    let stack_up = spell_with(
        STACK_UP,
        vec![Effect::ApplyAura {
            aura: BLEED_DOT,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
            per_unit_spent: None,
        }],
    );
    for s in [stacked, stack_up] {
        f.template.abilities.insert(s.id);
        data.spells.insert(s.id, s);
    }
    let mut k = kernel(&f, 1, 1, instant());
    in_combat(&mut k);
    for expected in [false, false, true] {
        assert_eq!(ready(&k, STACKED), expected);
        k.submit(Seat(0), cast(STACK_UP)).unwrap();
        let Step::Decide(_) = k.advance().unwrap() else {
            panic!("the run ended");
        };
    }
}

/// Readiness follows the primary target; a cast at another target must
/// meet the requirement too.
#[test]
fn a_cast_at_another_target_must_meet_the_requirement_itself() {
    let f = executioner(2);
    let mut k = kernel(&f, 1, 1, instant());
    in_combat(&mut k);
    let me = k.state().seats()[0];
    let primary = k.state().target(me).unwrap();
    let other = *k
        .state()
        .enemies()
        .iter()
        .find(|&&e| e != primary)
        .expect("a second dummy");
    for _ in 0..8 {
        k.submit(Seat(0), cast(CHIP)).unwrap();
        let Step::Decide(_) = k.advance().unwrap() else {
            panic!("the run ended");
        };
    }
    assert!(ready(&k, EXECUTE));
    let at = |t: ActorId| Choice::Cast {
        ability: EXECUTE,
        target: TargetSel::Actor(t),
        opts: CastOpts::default(),
    };
    assert_eq!(
        refusal(&mut k, at(other)),
        IllegalChoice::TargetRequirement(other)
    );
    k.submit(Seat(0), at(primary)).unwrap();
}

/// A seat waiting far ahead is woken, unanticipated, by the tick that
/// takes its target to 20%; one without a health-gated ability isn't.
#[test]
fn a_waiting_seat_is_woken_when_its_target_reaches_the_threshold() {
    let first_wake = |with_execute: bool| {
        let mut f = executioner(1);
        if !with_execute {
            f.template.abilities.remove(&EXECUTE);
        }
        let mut k = kernel(&f, 1, 1, instant());
        in_combat(&mut k);
        k.submit(Seat(0), cast(BLEED)).unwrap();
        let applied = k.state().now();
        let far = applied + SimDuration(600_000);
        loop {
            let Step::Decide(req) = k.advance().unwrap() else {
                return None;
            };
            if req.now > applied {
                return Some((
                    req.reason,
                    req.anticipated,
                    req.now.saturating_since(applied),
                ));
            }
            k.submit(Seat(0), Choice::Wait(Wait::Until(far))).unwrap();
        }
    };
    let (reason, anticipated, after) = first_wake(true).expect("a wake");
    assert!(matches!(reason, WakeReason::TargetHealth(_)), "{reason:?}");
    assert!(!anticipated);
    assert_eq!(after, SimDuration(16_000), "the 16th tick leaves 20%");
    assert_eq!(first_wake(false), None, "the dummy dies unannounced");
}
