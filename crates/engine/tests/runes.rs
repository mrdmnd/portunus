//! Resources that refill a unit at a time, a few units at once (Death
//! Knight runes): which units refill when, haste, and waiting for them.

mod common;

use std::sync::Arc;

use common::*;
use portunus_core::{ActorId, Seat, SimDuration, SimTime, SpellId};
use portunus_engine::choice::{CmpOp, Condition, Scalar};
use portunus_engine::mechanics::{
    AuraChange, AuraEvent, CastEvent, DeathEvent, EnemyHit, PetEvent, RolledHit, SwingEvent,
    TickEvent, TimerEvent,
};
use portunus_engine::state::Projectile;
use portunus_engine::{
    AuraRef, Choice, Engine, EngineIo, Kernel, Mechanics, Readiness, StateView, Step, Wait,
    WakeReason,
};
use portunus_gamedata::effect::Effect;
use portunus_gamedata::spell::{CastKind, SpellDef, Targeting};
use portunus_gamedata::stats::{
    Cost, RechargeDef, ResourceAmount, ResourceDef, ResourceKind, SpendScaling,
};

const SPEND: SpellId = SpellId(900_901);
const HASTE: SpellId = SpellId(900_902);
const PERIOD: SimDuration = SimDuration(10_000);
const TWICE: SimDuration = SimDuration(20_000);
const GAIN: SpellId = SpellId(900_903);
const BIG: SpellId = SpellId(900_904);

/// `Stub`, except that landing `HASTE` doubles the caster's haste.
#[derive(Debug, Clone)]
struct Hasty;

impl Mechanics for Hasty {
    fn gate(&self, view: &dyn StateView, seat: Seat, ability: SpellId) -> Readiness {
        Stub.gate(view, seat, ability)
    }
    fn combat_started(&self, io: &mut dyn EngineIo, combat: u16) {
        Stub.combat_started(io, combat);
    }
    fn combat_ended(&self, io: &mut dyn EngineIo, combat: u16, cleared: bool) {
        Stub.combat_ended(io, combat, cleared);
    }
    fn cast_started(&self, io: &mut dyn EngineIo, cast: &CastEvent) {
        Stub.cast_started(io, cast);
    }
    fn cast_completed(&self, io: &mut dyn EngineIo, cast: &CastEvent) {
        if cast.spell == HASTE {
            io.set_haste(cast.actor, 2.0);
        }
        Stub.cast_completed(io, cast);
    }
    fn channel_tick(&self, io: &mut dyn EngineIo, cast: &CastEvent, tick: u8) {
        Stub.channel_tick(io, cast, tick);
    }
    fn projectile_landed(
        &self,
        io: &mut dyn EngineIo,
        cast: &CastEvent,
        flight: &Projectile,
        hits: &[RolledHit],
    ) {
        Stub.projectile_landed(io, cast, flight, hits);
    }
    fn swing(&self, io: &mut dyn EngineIo, swing: &SwingEvent) {
        Stub.swing(io, swing);
    }
    fn periodic_tick(&self, io: &mut dyn EngineIo, tick: &TickEvent) {
        Stub.periodic_tick(io, tick);
    }
    fn aura_changed(&self, io: &mut dyn EngineIo, ev: &AuraChange) {
        Stub.aura_changed(io, ev);
    }
    fn aura_removed(&self, io: &mut dyn EngineIo, ev: &AuraEvent) {
        Stub.aura_removed(io, ev);
    }
    fn actor_died(&self, io: &mut dyn EngineIo, ev: &DeathEvent) {
        Stub.actor_died(io, ev);
    }
    fn pet_expired(&self, io: &mut dyn EngineIo, ev: &PetEvent) {
        Stub.pet_expired(io, ev);
    }
    fn aura_threshold(&self, io: &mut dyn EngineIo, aura: AuraRef) {
        Stub.aura_threshold(io, aura);
    }
    fn enemy_hit(&self, io: &mut dyn EngineIo, hit: &EnemyHit) {
        Stub.enemy_hit(io, hit);
    }
    fn enemy_effects(
        &self,
        io: &mut dyn EngineIo,
        source: ActorId,
        target: ActorId,
        effects: &[Effect],
    ) {
        Stub.enemy_effects(io, source, target, effects);
    }
    fn timer(&self, io: &mut dyn EngineIo, timer: &TimerEvent) {
        Stub.timer(io, timer);
    }
}

/// An instant, off-GCD spell at the enemy that does nothing itself.
fn spell(id: SpellId, costs: Vec<Cost>) -> SpellDef {
    let base = fixture().data.spells[&EARTH_SHOCK].clone();
    SpellDef {
        id,
        name: format!("spell {}", id.0),
        cast: CastKind::Instant,
        gcd: None,
        cooldown: None,
        costs,
        targeting: Targeting::Enemy,
        hostile: true,
        speed: None,
        min_travel: SimDuration::ZERO,
        requires: Vec::new(),
        effects: Vec::new(),
        ..base
    }
}

/// Six runes, three refilling at once over 10 s (hasted); Spend costs two, Big
/// four; Gain gives one.
fn rune_kernel(haste_pct: f64) -> Kernel<Hasty> {
    let mut f = fixture();
    f.template.derived.haste_pct = haste_pct;
    f.template.resources = vec![ResourceDef {
        kind: ResourceKind::Runes,
        max: 6.0,
        initial: 6.0,
        regen_per_sec: 0.0,
        regen_hasted: false,
        recharge: Some(RechargeDef {
            concurrent: 3,
            period: PERIOD,
            hasted: true,
        }),
        out_of_combat: None,
    }];
    let data = Arc::make_mut(&mut f.data);
    let runes = |amount: f64| Cost {
        kind: ResourceKind::Runes,
        amount,
        extra: 0.0,
        scaling: SpendScaling::None,
    };
    let mut gain = spell(GAIN, Vec::new());
    gain.effects = vec![Effect::Resource(ResourceAmount {
        kind: ResourceKind::Runes,
        amount: 1.0,
    })];
    for s in [
        spell(SPEND, vec![runes(2.0)]),
        spell(BIG, vec![runes(4.0)]),
        spell(HASTE, Vec::new()),
        gain,
    ] {
        f.template.abilities.insert(s.id);
        data.spells.insert(s.id, s);
    }
    let mut k = Kernel::new(setup(&f, 1, 1, instant()), Hasty).unwrap();
    loop {
        let Step::Decide(req) = k.advance().unwrap() else {
            panic!("the run ended before combat");
        };
        let me = k.state().seats()[0];
        if k.state().target(me).is_some() && readiness(&k, SPEND) == Readiness::Now {
            return k;
        }
        k.submit(req.seat, Choice::Wait(Wait::NextEvent)).unwrap();
    }
}

fn readiness(k: &Kernel<Hasty>, ability: SpellId) -> Readiness {
    k.legal(Seat(0)).abilities[&ability]
}

/// The seat's runes now, and when each missing one is back.
fn runes(k: &Kernel<Hasty>) -> (f64, Vec<SimTime>) {
    let me = k.state().seats()[0];
    let r = k.state().resource(me, ResourceKind::Runes).unwrap();
    (r.value, r.next_ready)
}

/// Submit a choice and take the next decision, which must come at once.
fn now_then(k: &mut Kernel<Hasty>, choice: Choice) {
    let at = k.state().now();
    k.submit(Seat(0), choice).unwrap();
    let Step::Decide(req) = k.advance().unwrap() else {
        panic!("the run ended");
    };
    assert_eq!(req.now, at, "{:?}", req.reason);
}

#[test]
fn spent_runes_refill_three_at_a_time() {
    let mut k = rune_kernel(0.0);
    let t0 = k.state().now();
    now_then(&mut k, cast(SPEND));
    assert_eq!(runes(&k), (4.0, vec![t0 + PERIOD; 2]));
    now_then(&mut k, cast(SPEND));
    assert_eq!(
        runes(&k),
        (2.0, vec![t0 + PERIOD, t0 + PERIOD, t0 + PERIOD, t0 + TWICE]),
        "the fourth waits for one of the first three"
    );
    now_then(&mut k, cast(SPEND));
    let mut expected = vec![t0 + PERIOD; 3];
    expected.extend([t0 + TWICE; 3]);
    assert_eq!(runes(&k), (0.0, expected));
    assert_eq!(readiness(&k, SPEND), Readiness::In(PERIOD));
    assert_eq!(
        readiness(&k, BIG),
        Readiness::In(TWICE),
        "the fourth rune back"
    );
}

#[test]
fn waiting_for_runes_wakes_when_enough_are_back() {
    let mut k = rune_kernel(0.0);
    let t0 = k.state().now();
    for _ in 0..3 {
        now_then(&mut k, cast(SPEND));
    }
    let three = Condition::Cmp {
        lhs: Scalar::Resource(ResourceKind::Runes),
        op: CmpOp::Ge,
        rhs: 3.0,
    };
    k.submit(Seat(0), Choice::Wait(Wait::Condition(three)))
        .unwrap();
    let Step::Decide(req) = k.advance().unwrap() else {
        panic!("the run ended");
    };
    assert_eq!(req.reason, WakeReason::ConditionMet);
    assert_eq!(req.now, t0 + PERIOD);
    assert_eq!(runes(&k), (3.0, vec![t0 + TWICE; 3]));
}

/// SimC's `replenish_rune`: a gained rune fills a spent one that isn't
/// refilling, and only then one that is.
#[test]
fn a_gained_rune_fills_an_idle_one_first() {
    let mut k = rune_kernel(0.0);
    let t0 = k.state().now();
    for _ in 0..3 {
        now_then(&mut k, cast(SPEND));
    }
    let halfway = t0 + SimDuration(5_000);
    k.submit(Seat(0), Choice::Wait(Wait::Until(halfway)))
        .unwrap();
    let Step::Decide(req) = k.advance().unwrap() else {
        panic!("the run ended");
    };
    assert_eq!(req.now, halfway);
    now_then(&mut k, cast(GAIN));
    let mut expected = vec![t0 + PERIOD; 3];
    expected.extend([t0 + TWICE; 2]);
    assert_eq!(runes(&k), (1.0, expected));
    now_then(&mut k, cast(GAIN));
    now_then(&mut k, cast(GAIN));
    assert_eq!(runes(&k), (3.0, vec![t0 + PERIOD; 3]));
    now_then(&mut k, cast(GAIN));
    assert_eq!(runes(&k), (4.0, vec![t0 + PERIOD; 2]));
}

#[test]
fn haste_shortens_the_refill() {
    let mut k = rune_kernel(30.0);
    let t0 = k.state().now();
    now_then(&mut k, cast(SPEND));
    let hasted = t0 + SimDuration(7_693);
    assert_eq!(runes(&k), (4.0, vec![hasted; 2]), "10 s / 1.3, rounded up");
}

#[test]
fn a_haste_change_rescales_what_is_left() {
    let mut k = rune_kernel(0.0);
    let t0 = k.state().now();
    now_then(&mut k, cast(SPEND));
    let halfway = t0 + SimDuration(5_000);
    k.submit(Seat(0), Choice::Wait(Wait::Until(halfway)))
        .unwrap();
    let Step::Decide(req) = k.advance().unwrap() else {
        panic!("the run ended");
    };
    assert_eq!(req.now, halfway);
    now_then(&mut k, cast(HASTE));
    let doubled = halfway + SimDuration(2_500);
    assert_eq!(
        runes(&k),
        (4.0, vec![doubled; 2]),
        "5 s left at double speed"
    );
}
