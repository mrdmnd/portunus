//! The party-level [`Mechanics`].

use std::borrow::Cow;
use std::sync::Arc;

use portunus_core::{ActorId, Seat, SpellId};
use portunus_engine::mechanics::{
    whole_points, AbsorbEvent, AuraChange, AuraEvent, AuraRemoval, CastEvent, DamageEvent,
    DeathEvent, EnemyHit, HitKind, PetEvent, RolledHit, SwingEvent, TickEvent, TimerEvent,
};
use portunus_engine::state::{ActorKind, Projectile};
use portunus_engine::{AuraRef, EngineIo, Mechanics, Readiness, RunSetup, StateView};
use portunus_gamedata::aura::{AuraDef, AuraValueKind, BankDraw};
use portunus_gamedata::effect::{Effect, ModKind, RemovalReason};
use portunus_gamedata::spell::{CastKind, SpellDef};
use portunus_gamedata::stats::{SpendScaling, Stat};

use crate::interp::{health_fraction, Happening, Interpreter, Occurrence};
use crate::math::{instance, owner_seat, player_seat, Formulas};
use crate::{CombatMath, EffectCtx, EffectInterpreter, IncomingHit, KitIssue, SpecRegistry};

/// Game semantics for a whole party: data effects through the
/// [`Interpreter`], formulas from [`Formulas`], and each seat's spec kit
/// for gates, hooks, and timers.
///
/// A spell's effects run when it completes, or when it lands if it travels;
/// a channel's run on each tick, and a released empower's are followed by
/// its stage's. Its `CastComplete` and `ResourceSpent` listeners fire at
/// completion either way. Effects run by a listener don't fire further
/// listeners.
#[derive(Clone)]
pub struct PartyMechanics {
    interp: Interpreter,
}

/// What a cast runs: the spell's effects, then a released empower's
/// stage effects (SimC folds the stage into the release spell's own
/// numbers; the data spells the difference out per stage).
fn cast_effects<'a>(def: &'a SpellDef, cast: &CastEvent) -> Cow<'a, [Effect]> {
    let extra = match (&def.cast, cast.empower) {
        (CastKind::Empower { stage_effects, .. }, Some(stage)) => usize::from(stage)
            .checked_sub(1)
            .and_then(|i| stage_effects.get(i)),
        _ => None,
    };
    match extra {
        Some(extra) if !extra.is_empty() => {
            Cow::Owned(def.effects.iter().chain(extra).cloned().collect())
        }
        _ => Cow::Borrowed(&def.effects),
    }
}

impl PartyMechanics {
    /// Fails with every problem if the kits don't cover the data.
    pub fn new(setup: &RunSetup, kits: Arc<dyn SpecRegistry>) -> Result<Self, Vec<KitIssue>> {
        let mut issues = kits.validate(&setup.data);
        for seat in &setup.seats {
            let spec = seat.template.spec;
            if kits.kit(spec).is_none() && !issues.contains(&KitIssue::MissingKit(spec)) {
                issues.push(KitIssue::MissingKit(spec));
            }
        }
        if !issues.is_empty() {
            return Err(issues);
        }
        Ok(Self {
            interp: Interpreter::new(Formulas::new(setup), kits),
        })
    }

    pub fn math(&self) -> &Formulas {
        self.interp.math()
    }

    fn spell(&self, id: SpellId) -> Option<&SpellDef> {
        self.math().data().spells.get(&id)
    }

    fn aura_def(&self, id: portunus_core::AuraId) -> Option<&AuraDef> {
        self.math().data().auras.get(&id)
    }

    /// Take a bank tick's share out of its value and return it. Spread
    /// banks follow SimC's `residual_action`: the pool over the ticks left,
    /// so a refresh rolls what's left over the new duration and the final
    /// tick empties it. SimC fixes the per-tick share when the bank is
    /// added to; recomputing it each tick only differs if the source's
    /// haste changes the tick count mid-bank, and then still empties it.
    fn draw_bank(&self, io: &mut dyn EngineIo, tick: &TickEvent, draw: BankDraw) -> f64 {
        let r = tick.aura;
        let Some(value) = instance(io.view(), r).map(|i| i.value) else {
            return 0.0;
        };
        let share = match draw {
            BankDraw::SpreadOverRemaining => match tick.ticks_left {
                Some(left) if left > tick.fraction => tick.fraction / left,
                _ => 1.0,
            },
            BankDraw::Fraction(f) => f * tick.fraction,
        };
        let drawn = value.max(0.0) * share.clamp(0.0, 1.0);
        let limits = self.interp.value_limits(io.view(), r);
        io.add_aura_value(r, -drawn, limits);
        drawn
    }

    fn spell_ctx(&self, cast: &CastEvent) -> EffectCtx {
        EffectCtx {
            caster: cast.actor,
            target: cast.target,
            spell: Some(cast.spell),
            aura: None,
            event_amount: None,
            scale: self.spend_scale(cast),
            spent: cast.spent.map(|s| s.amount),
            depth: 0,
            hand: self.spell(cast.spell).and_then(|d| d.weapon),
            hit: HitKind::Direct,
            ground: None,
        }
    }

    fn spend_scale(&self, cast: &CastEvent) -> f64 {
        let (Some(def), Some(spent)) = (self.spell(cast.spell), cast.spent) else {
            return 1.0;
        };
        let Some(cost) = def.costs.iter().find(|c| c.kind == spent.kind) else {
            return 1.0;
        };
        match cost.scaling {
            SpendScaling::None => 1.0,
            SpendScaling::PerUnit => spent.amount,
            SpendScaling::Extra(coef) if cost.extra > 0.0 => {
                1.0 + coef * ((spent.amount - cost.amount) / cost.extra).clamp(0.0, 1.0)
            }
            SpendScaling::Extra(_) => 1.0,
        }
    }

    /// Re-push haste and attack speed to the kernel if this aura can change
    /// them. A seat's haste reaches its pets too.
    fn refresh_speed(&self, io: &mut dyn EngineIo, holder: ActorId, def: &AuraDef) {
        let touches_haste = def.modifiers.iter().any(|m| {
            matches!(
                m.kind,
                ModKind::HastePct
                    | ModKind::StatFlat(Stat::HasteRating)
                    | ModKind::StatPct(Stat::HasteRating)
            )
        });
        let touches_attack_speed = def
            .modifiers
            .iter()
            .any(|m| m.kind == ModKind::AttackSpeedPct);
        let view = io.view();
        let mut hasted = Vec::new();
        if touches_haste {
            if let Some(seat) = player_seat(view, holder) {
                hasted.push(holder);
                hasted.extend_from_slice(view.pets(seat));
            } else if owner_seat(view, holder).is_some() {
                hasted.push(holder);
            }
        }
        let hasted: Vec<(ActorId, f64)> = hasted
            .into_iter()
            .map(|a| (a, self.math().haste_mult(view, a)))
            .collect();
        let attack_speed = (touches_attack_speed && owner_seat(view, holder).is_some())
            .then(|| self.math().attack_speed_mult(view, holder));
        for (actor, mult) in hasted {
            io.set_haste(actor, mult);
        }
        if let Some(mult) = attack_speed {
            io.set_attack_speed(holder, mult);
        }
    }
}

impl Mechanics for PartyMechanics {
    fn gate(&self, view: &dyn StateView, seat: Seat, ability: SpellId) -> Readiness {
        self.math()
            .seat_spec(seat)
            .and_then(|spec| self.interp.kits().kit(spec))
            .map_or(Readiness::Now, |kit| kit.gate(view, seat, ability))
    }

    fn combat_started(&self, _io: &mut dyn EngineIo, _combat: u16) {}

    fn combat_ended(&self, _io: &mut dyn EngineIo, _combat: u16, _cleared: bool) {}

    fn cast_started(&self, io: &mut dyn EngineIo, cast: &CastEvent) {
        let Some(def) = self.spell(cast.spell) else {
            return;
        };
        if matches!(def.cast, CastKind::Instant | CastKind::Channel { .. }) {
            return;
        }
        let started = Occurrence {
            what: Happening::CastStart {
                spell: cast.spell,
                school: def.school,
            },
            target: cast.target,
            amount: None,
            depth: 0,
        };
        self.interp.fire(io, cast.actor, started);
    }

    fn cast_completed(&self, io: &mut dyn EngineIo, cast: &CastEvent) {
        let Some(def) = self.spell(cast.spell) else {
            return;
        };
        let ctx = self.spell_ctx(cast);
        let channel = matches!(def.cast, CastKind::Channel { .. });
        if !cast.prerolled && !channel {
            let effects = cast_effects(def, cast);
            if !def.travels() {
                self.interp.run(io, &ctx, &effects);
            } else if !def.rolls_on_impact {
                for hit in self.interp.roll_direct(io, &ctx, &effects) {
                    io.stash_hit(hit);
                }
            }
        }
        let done = Occurrence {
            what: Happening::CastComplete {
                spell: cast.spell,
                school: def.school,
            },
            target: cast.target,
            amount: None,
            depth: 0,
        };
        self.interp.fire(io, cast.actor, done);
        for cost in &def.costs {
            let amount = cast
                .spent
                .filter(|s| s.kind == cost.kind)
                .map_or(cost.amount, |s| s.amount);
            let spent = Occurrence {
                what: Happening::ResourceSpent(cost.kind),
                amount: Some(amount),
                ..done
            };
            self.interp.fire(io, cast.actor, spent);
        }
    }

    /// The channel's effects, as periodic hits: SimC assesses a channel's
    /// own ticks as `DMG_OVER_TIME`, so armor doesn't apply. A spell a tick
    /// triggers (SimC's `tick_action`) hits as direct.
    fn channel_tick(&self, io: &mut dyn EngineIo, cast: &CastEvent, _tick: u8) {
        let Some(def) = self.spell(cast.spell) else {
            return;
        };
        let ctx = EffectCtx {
            hit: HitKind::Periodic,
            ..self.spell_ctx(cast)
        };
        self.interp.run(io, &ctx, &def.effects);
    }

    fn projectile_landed(
        &self,
        io: &mut dyn EngineIo,
        cast: &CastEvent,
        _flight: &Projectile,
        hits: &[RolledHit],
    ) {
        if let Some(def) = self.spell(cast.spell) {
            let ctx = self.spell_ctx(cast);
            let effects = cast_effects(def, cast);
            if def.rolls_on_impact {
                self.interp.run(io, &ctx, &effects);
            } else {
                self.interp.land(io, &ctx, &effects, hits);
            }
        }
    }

    /// A white hit at the swing's target, then `Swing` listeners.
    fn swing(&self, io: &mut dyn EngineIo, swing: &SwingEvent) {
        self.interp
            .white_hit(io, swing.actor, swing.hand, swing.target, 0);
    }

    fn periodic_tick(&self, io: &mut dyn EngineIo, tick: &TickEvent) {
        let r = tick.aura;
        let Some(def) = self.aura_def(r.aura) else {
            return;
        };
        let Some(periodic) = def.periodic.as_ref() else {
            return;
        };
        let drawn = match def.value.as_ref().map(|v| v.kind) {
            Some(AuraValueKind::Bank(draw)) => Some(self.draw_bank(io, tick, draw)),
            _ => None,
        };
        let spell = SpellId(r.aura.0);
        let ctx = EffectCtx {
            caster: r.source,
            target: Some(r.holder),
            spell: self.spell(spell).map(|_| spell),
            aura: Some(r),
            event_amount: drawn,
            // A bank's draw already shrinks for a partial final tick.
            scale: if drawn.is_some() { 1.0 } else { tick.fraction },
            spent: None,
            depth: 0,
            hand: None,
            hit: HitKind::Periodic,
            ground: io
                .view()
                .auras(r.holder)
                .iter()
                .find(|i| i.aura == r.aura && i.source == r.source)
                .and_then(|i| i.ground),
        };
        self.interp.run(io, &ctx, &periodic.effects);
        let ticked = Occurrence {
            what: Happening::PeriodicTick(r.aura),
            target: Some(r.holder),
            amount: None,
            depth: 0,
        };
        self.interp.fire(io, r.source, ticked);
    }

    fn aura_changed(&self, io: &mut dyn EngineIo, ev: &AuraChange) {
        let Some(def) = self.aura_def(ev.aura.aura) else {
            return;
        };
        if ev.previous_stacks == 0 {
            if let Some(initial) = def.value.as_ref().and_then(|v| v.initial) {
                let ctx = self.interp.aura_ctx(ev.aura);
                let amount = self.math().base(io.view(), &ctx, initial);
                let limits = self.interp.value_limits(io.view(), ev.aura);
                io.add_aura_value(ev.aura, amount, limits);
            }
            let applied = Occurrence {
                what: Happening::AuraApplied(ev.aura.aura),
                target: Some(ev.aura.holder),
                amount: None,
                depth: 0,
            };
            self.interp.fire(io, ev.aura.holder, applied);
        }
        if ev.stacks > ev.previous_stacks {
            let stacked = Occurrence {
                what: Happening::AuraStacks {
                    aura: ev.aura.aura,
                    from: ev.previous_stacks,
                    to: ev.stacks,
                },
                target: Some(ev.aura.holder),
                amount: None,
                depth: 0,
            };
            self.interp.fire(io, ev.aura.holder, stacked);
        }
        self.refresh_speed(io, ev.aura.holder, def);
    }

    fn aura_removed(&self, io: &mut dyn EngineIo, ev: &AuraEvent) {
        let r = ev.aura;
        let Some(def) = self.aura_def(r.aura) else {
            return;
        };
        if ev.reason == AuraRemoval::Expired && !def.on_expire.is_empty() {
            let ctx = EffectCtx {
                caster: r.source,
                target: Some(r.holder),
                spell: None,
                aura: Some(r),
                event_amount: None,
                scale: 1.0,
                spent: None,
                depth: 0,
                hand: None,
                hit: HitKind::Direct,
                ground: ev.ground,
            };
            self.interp.run(io, &ctx, &def.on_expire);
        }
        if let (AuraRemoval::Broken, Some(st)) = (ev.reason, &def.stealth) {
            let ctx = EffectCtx {
                caster: r.holder,
                target: Some(r.holder),
                spell: None,
                aura: Some(r),
                event_amount: None,
                scale: 1.0,
                spent: None,
                depth: 0,
                hand: None,
                hit: HitKind::Direct,
                ground: None,
            };
            self.interp.run(io, &ctx, &st.on_break);
        }
        if ev.reason != AuraRemoval::HolderDied {
            let expired = Occurrence {
                what: Happening::AuraExpired(r.aura),
                target: Some(r.holder),
                amount: None,
                depth: 0,
            };
            self.interp.fire(io, r.holder, expired);
        }
        let reason = match ev.reason {
            AuraRemoval::Expired => Some(RemovalReason::Expired),
            AuraRemoval::Removed => Some(RemovalReason::Removed),
            AuraRemoval::Depleted => Some(RemovalReason::Depleted),
            AuraRemoval::Broken => Some(RemovalReason::Broken),
            AuraRemoval::HolderDied => None,
        };
        if let Some(reason) = reason {
            let removed = Occurrence {
                what: Happening::AuraRemoved {
                    aura: r.aura,
                    reason,
                },
                target: Some(r.holder),
                amount: None,
                depth: 0,
            };
            self.interp.fire(io, r.holder, removed);
        }
        self.refresh_speed(io, r.holder, def);
    }

    /// An enemy's death reaches every seat's and pet's listeners.
    fn actor_died(&self, io: &mut dyn EngineIo, ev: &DeathEvent) {
        let view = io.view();
        if !matches!(
            view.actor(ev.actor).map(|a| a.kind),
            Some(ActorKind::Enemy { .. })
        ) {
            return;
        }
        let holders: Vec<ActorId> = view
            .seats()
            .iter()
            .enumerate()
            .flat_map(|(i, &me)| {
                std::iter::once(me).chain(view.pets(Seat(i as u8)).iter().copied())
            })
            .collect();
        for holder in holders {
            let died = Occurrence {
                what: Happening::EnemyDied {
                    by_holder: ev.killer == Some(holder),
                },
                target: Some(ev.actor),
                amount: None,
                depth: 0,
            };
            self.interp.fire(io, holder, died);
        }
    }

    fn aura_threshold(&self, io: &mut dyn EngineIo, aura: AuraRef) {
        let Some(v) = self.aura_def(aura.aura).and_then(|d| d.value.as_ref()) else {
            return;
        };
        let ctx = self.interp.aura_ctx(aura);
        self.interp.run(io, &ctx, &v.on_threshold);
    }

    fn pet_expired(&self, io: &mut dyn EngineIo, ev: &PetEvent) {
        let Some(&owner) = io.view().seats().get(usize::from(ev.owner.0)) else {
            return;
        };
        let departed = Occurrence {
            what: Happening::Departed,
            target: io.view().target(owner),
            amount: None,
            depth: 0,
        };
        self.interp.fire(io, ev.actor, departed);
        let expired = Occurrence {
            what: Happening::PetExpired(ev.pet),
            ..departed
        };
        self.interp.fire(io, owner, expired);
    }

    fn enemy_hit(&self, io: &mut dyn EngineIo, hit: &EnemyHit) {
        let amount = self.math().mitigate(
            io.view(),
            hit.target,
            hit.amount,
            IncomingHit::direct(hit.school),
        );
        let before = health_fraction(io.view(), hit.target);
        let landed = io.apply_damage(DamageEvent {
            source: hit.source,
            target: hit.target,
            amount: whole_points(amount),
            school: hit.school,
            spell: None,
            crit: false,
        });
        let health = (before, health_fraction(io.view(), hit.target));
        if landed.connected() {
            let taken = Occurrence {
                what: Happening::DamageTaken,
                target: Some(hit.source),
                amount: Some(landed.amount as f64),
                depth: 0,
            };
            self.interp.fire(io, hit.target, taken);
            self.interp
                .health_dropped(io, hit.target, health, hit.source, 0);
        }
    }

    fn enemy_effects(
        &self,
        io: &mut dyn EngineIo,
        source: ActorId,
        target: ActorId,
        effects: &[Effect],
    ) {
        let ctx = EffectCtx {
            caster: source,
            target: Some(target),
            spell: None,
            aura: None,
            event_amount: None,
            scale: 1.0,
            spent: None,
            depth: 0,
            hand: None,
            hit: HitKind::Direct,
            ground: None,
        };
        self.interp.run(io, &ctx, effects);
    }

    fn timer(&self, io: &mut dyn EngineIo, timer: &TimerEvent) {
        let kit = player_seat(io.view(), timer.owner)
            .and_then(|seat| self.math().seat_spec(seat))
            .and_then(|spec| self.interp.kits().kit(spec));
        if let Some(kit) = kit {
            kit.timer(self.interp.tools(), io, timer);
        }
    }

    /// The shield's caster hears of it.
    fn aura_absorbed(&self, io: &mut dyn EngineIo, ev: &AbsorbEvent) {
        let soaked = Occurrence {
            what: Happening::Absorbed(ev.aura.aura),
            target: Some(ev.aura.holder),
            amount: Some(ev.amount as f64),
            depth: 0,
        };
        self.interp.fire(io, ev.aura.source, soaked);
    }

    fn movement_changed(&self, io: &mut dyn EngineIo, seat: Seat, moving: bool) {
        let Some(&me) = io.view().seats().get(usize::from(seat.0)) else {
            return;
        };
        let moved = Occurrence {
            what: if moving {
                Happening::MoveStart
            } else {
                Happening::MoveEnd
            },
            target: io.view().target(me),
            amount: None,
            depth: 0,
        };
        self.interp.fire(io, me, moved);
    }

    fn death_prevented(&self, io: &mut dyn EngineIo, aura: AuraRef) {
        let Some(p) = self
            .aura_def(aura.aura)
            .and_then(|d| d.prevents_death.as_ref())
        else {
            return;
        };
        let ctx = self.interp.aura_ctx(aura);
        self.interp.run(io, &ctx, &p.on_prevent);
    }
}
