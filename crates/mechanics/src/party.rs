//! The party-level [`Mechanics`].

use std::sync::Arc;

use portunus_core::{ActorId, Seat, SpellId};
use portunus_engine::mechanics::{
    whole_points, AuraChange, AuraEvent, AuraRemoval, CastEvent, DamageEvent, DeathEvent, EnemyHit,
    PetEvent, RolledHit, SwingEvent, TickEvent, TimerEvent,
};
use portunus_engine::state::Projectile;
use portunus_engine::{EngineIo, Mechanics, Readiness, RunSetup, StateView};
use portunus_gamedata::aura::AuraDef;
use portunus_gamedata::effect::{Coefficient, EffectTarget, ModKind};
use portunus_gamedata::spell::SpellDef;
use portunus_gamedata::stats::{SchoolMask, SpendScaling, Stat};

use crate::interp::{Happening, Interpreter, Occurrence};
use crate::math::{owner_seat, player_seat, Formulas};
use crate::{CombatMath, EffectCtx, EffectInterpreter, KitIssue, SpecRegistry};

/// Game semantics for a whole party: data effects through the
/// [`Interpreter`], formulas from [`Formulas`], and each seat's spec kit
/// for gates, hooks, and timers.
///
/// A spell's effects run when it completes, or when it lands if it travels.
/// Its `CastComplete` and `ResourceSpent` listeners fire at completion
/// either way. Effects run by a listener don't fire further listeners.
#[derive(Clone)]
pub struct PartyMechanics {
    interp: Interpreter,
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

    fn spell_ctx(&self, cast: &CastEvent) -> EffectCtx {
        EffectCtx {
            caster: cast.actor,
            target: cast.target,
            spell: Some(cast.spell),
            aura: None,
            event_amount: None,
            scale: self.spend_scale(cast),
            depth: 0,
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

    fn cast_started(&self, _io: &mut dyn EngineIo, _cast: &CastEvent) {}

    fn cast_completed(&self, io: &mut dyn EngineIo, cast: &CastEvent) {
        let Some(def) = self.spell(cast.spell) else {
            return;
        };
        let ctx = self.spell_ctx(cast);
        if !cast.prerolled {
            if !def.travels() {
                self.interp.run(io, &ctx, &def.effects);
            } else if !def.rolls_on_impact {
                for hit in self.interp.roll_direct(io, &ctx, &def.effects) {
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

    fn channel_tick(&self, _io: &mut dyn EngineIo, _cast: &CastEvent, _tick: u8) {}

    fn projectile_landed(
        &self,
        io: &mut dyn EngineIo,
        cast: &CastEvent,
        _flight: &Projectile,
        hits: &[RolledHit],
    ) {
        if let Some(def) = self.spell(cast.spell) {
            let ctx = self.spell_ctx(cast);
            if def.rolls_on_impact {
                self.interp.run(io, &ctx, &def.effects);
            } else {
                self.interp.land(io, &ctx, &def.effects, hits);
            }
        }
    }

    /// A white hit at the swing's target, then `Swing` listeners.
    fn swing(&self, io: &mut dyn EngineIo, swing: &SwingEvent) {
        let raw = self
            .math()
            .weapon_damage(io.view(), swing.actor, swing.hand);
        let ctx = EffectCtx {
            caster: swing.actor,
            target: Some(swing.target),
            spell: None,
            aura: None,
            event_amount: None,
            scale: 1.0,
            depth: 0,
        };
        self.interp.damage(
            io,
            &ctx,
            Coefficient::Flat(raw),
            SchoolMask::PHYSICAL,
            EffectTarget::Target,
            None,
        );
        let swung = Occurrence {
            what: Happening::Swing(swing.hand),
            target: Some(swing.target),
            amount: None,
            depth: 0,
        };
        self.interp.fire(io, swing.actor, swung);
    }

    fn periodic_tick(&self, io: &mut dyn EngineIo, tick: &TickEvent) {
        let r = tick.aura;
        let Some(periodic) = self.aura_def(r.aura).and_then(|d| d.periodic.as_ref()) else {
            return;
        };
        let spell = SpellId(r.aura.0);
        let ctx = EffectCtx {
            caster: r.source,
            target: Some(r.holder),
            spell: self.spell(spell).map(|_| spell),
            aura: Some(r),
            event_amount: None,
            scale: tick.fraction,
            depth: 0,
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
            let applied = Occurrence {
                what: Happening::AuraApplied(ev.aura.aura),
                target: Some(ev.aura.holder),
                amount: None,
                depth: 0,
            };
            self.interp.fire(io, ev.aura.holder, applied);
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
                depth: 0,
            };
            self.interp.run(io, &ctx, &def.on_expire);
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
        self.refresh_speed(io, r.holder, def);
    }

    fn actor_died(&self, _io: &mut dyn EngineIo, _ev: &DeathEvent) {}

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
        let amount = self
            .math()
            .mitigate(io.view(), hit.target, hit.amount, hit.school);
        let landed = io.apply_damage(DamageEvent {
            source: hit.source,
            target: hit.target,
            amount: whole_points(amount),
            school: hit.school,
            spell: None,
            crit: false,
        });
        if landed > 0 {
            let taken = Occurrence {
                what: Happening::DamageTaken,
                target: Some(hit.source),
                amount: Some(landed as f64),
                depth: 0,
            };
            self.interp.fire(io, hit.target, taken);
        }
    }

    fn timer(&self, io: &mut dyn EngineIo, timer: &TimerEvent) {
        let kit = player_seat(io.view(), timer.owner)
            .and_then(|seat| self.math().seat_spec(seat))
            .and_then(|spec| self.interp.kits().kit(spec));
        if let Some(kit) = kit {
            kit.timer(self.interp.tools(), io, timer);
        }
    }
}
