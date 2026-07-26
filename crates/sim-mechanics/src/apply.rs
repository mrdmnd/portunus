//! The effect-IR interpreter: implements the kernel's [`Mechanics`] socket.
//!
//! `IrMechanics` holds only immutable compiled data (the op arena); every
//! mutation goes through [`EngineIo`] into `SimState` and the event queue,
//! preserving snapshot-as-clone, determinism, and CRN pairing.

use sim_engine::{EngineIo, EngineError, GenGuard, Mechanics, Priority, WakeReason};
use sim_engine::event::EventPayload;
use sim_types::{ActorId, AuraDef, AuraId, CompiledActor, EffectRange, SlotId};

use crate::ops::{EffectOp, OpArena};

/// Interpreter over the shared op arena.
#[derive(Debug, Clone)]
pub struct IrMechanics {
    arena: OpArena,
}

/// Who is acting on whom while a range executes.
#[derive(Debug, Clone, Copy)]
struct EffectCtx {
    source: ActorId,
    target: ActorId,
}

impl IrMechanics {
    #[must_use]
    pub fn new(arena: OpArena) -> Self {
        Self { arena }
    }

    fn source_kit<'r>(
        io: &EngineIo<'r>,
        actor: ActorId,
    ) -> Result<&'r CompiledActor, EngineError> {
        match io.state.actors[usize::from(actor.0)].kind {
            sim_engine::ActorKind::Player { roster_idx } => Ok(&io.roster[roster_idx]),
            sim_engine::ActorKind::Enemy => Err(EngineError::Mechanics(format!(
                "actor {} has no codex kit",
                actor.0
            ))),
        }
    }

    fn aura_def<'r>(
        io: &EngineIo<'r>,
        source: ActorId,
        aura: AuraId,
    ) -> Result<&'r AuraDef, EngineError> {
        Self::source_kit(io, source)?
            .auras
            .get(usize::from(aura.0))
            .ok_or_else(|| EngineError::Mechanics(format!("unknown aura {}", aura.0)))
    }

    /// Recompute a holder's resource regen rates from base rate times the
    /// product of all active aura multipliers (recompute-from-scratch keeps
    /// this order-independent and exact).
    fn recompute_rates(io: &mut EngineIo<'_>, holder: ActorId) -> Result<(), EngineError> {
        let Ok(kit) = Self::source_kit(io, holder) else {
            return Ok(()); // enemies have no resources
        };
        let auras = io.state.actors[usize::from(holder.0)].auras.clone();
        let now = io.state.now;
        for (ri, rdef) in kit.resources.iter().enumerate() {
            let mut rate = rdef.regen_per_sec;
            for slot in auras.iter().filter(|s| s.active) {
                let def = Self::aura_def(io, slot.source, slot.aura)?;
                if let Some((res, mult)) = def.rate_mult {
                    if usize::from(res.0) == ri {
                        rate *= mult;
                    }
                }
            }
            io.state.actors[usize::from(holder.0)].resources[ri].set_rate(now, rate);
        }
        Ok(())
    }

    fn apply_aura(
        &self,
        io: &mut EngineIo<'_>,
        source: ActorId,
        holder: ActorId,
        aura: AuraId,
    ) -> Result<(), EngineError> {
        let def = Self::aura_def(io, source, aura)?.clone();
        let now = io.state.now;
        let refreshed = io.state.actors[usize::from(holder.0)]
            .find_aura(aura, source)
            .is_some();

        // Claim bumps the slot generation, lazily invalidating the previous
        // expiry event and tick train (invariant 5) — this is the refresh path.
        let (slot_idx, gen) =
            io.state.actors[usize::from(holder.0)].claim_aura_slot(aura, source);
        {
            let slot = &mut io.state.actors[usize::from(holder.0)].auras[slot_idx];
            slot.active = true;
            slot.expires_at = now + def.duration;
            slot.stacks = if refreshed { slot.stacks.saturating_add(1) } else { 1 };
        }

        io.sched.schedule(
            now + def.duration,
            Priority::AuraExpiry,
            EventPayload::AuraExpire { holder, slot: slot_idx },
            Some(GenGuard::Aura { holder, slot: slot_idx, gen }),
        );
        if let Some(tick) = def.tick {
            io.sched.schedule(
                now + tick.period,
                Priority::PeriodicTick,
                EventPayload::PeriodicTick { holder, slot: slot_idx },
                Some(GenGuard::Aura { holder, slot: slot_idx, gen }),
            );
        }
        if def.rate_mult.is_some() {
            Self::recompute_rates(io, holder)?;
        }
        Ok(())
    }

    fn run_range(
        &self,
        io: &mut EngineIo<'_>,
        ctx: EffectCtx,
        range: EffectRange,
    ) -> Result<(), EngineError> {
        // Ranges are immutable compiled data; copy out to sidestep aliasing
        // with the mutable engine IO.
        let ops: Vec<EffectOp> = self.arena.range(range).to_vec();
        for op in ops {
            match op {
                EffectOp::DealDamage { amount } => {
                    io.deal_damage(ctx.source, ctx.target, amount);
                }
                EffectOp::ApplyAura { aura, on_self } => {
                    let holder = if on_self { ctx.source } else { ctx.target };
                    self.apply_aura(io, ctx.source, holder, aura)?;
                    if on_self {
                        // A self-buff can change usability: let the owner react.
                        io.request_wake(ctx.source, WakeReason::ProcApplied(aura));
                    }
                }
                EffectOp::GrantResource { resource, amount } => {
                    let now = io.state.now;
                    let a = &mut io.state.actors[usize::from(ctx.source.0)];
                    let res = a.resources.get_mut(usize::from(resource.0)).ok_or_else(|| {
                        EngineError::Mechanics(format!("unknown resource {}", resource.0))
                    })?;
                    res.add(now, amount);
                }
                EffectOp::SpendResource { resource, amount } => {
                    let now = io.state.now;
                    let a = &mut io.state.actors[usize::from(ctx.source.0)];
                    let res = a.resources.get_mut(usize::from(resource.0)).ok_or_else(|| {
                        EngineError::Mechanics(format!("unknown resource {}", resource.0))
                    })?;
                    res.add(now, -amount);
                }
                EffectOp::TriggerProc { chance, stream, effect } => {
                    // Semantic stream + per-(actor, stream) occurrence counter:
                    // draw order elsewhere cannot perturb this roll (invariant 6).
                    let roll = io.rng_uniform(ctx.source, stream);
                    if roll < chance {
                        self.run_range(io, ctx, effect)?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl Mechanics for IrMechanics {
    fn cast_complete(
        &self,
        io: &mut EngineIo<'_>,
        actor: ActorId,
        slot: SlotId,
        target: ActorId,
    ) -> Result<(), EngineError> {
        let range = Self::source_kit(io, actor)?
            .abilities
            .get(usize::from(slot.0))
            .ok_or(EngineError::UnknownSlot { actor, slot })?
            .effect;
        self.run_range(io, EffectCtx { source: actor, target }, range)
    }

    fn periodic_tick(
        &self,
        io: &mut EngineIo<'_>,
        holder: ActorId,
        aura_slot: usize,
    ) -> Result<(), EngineError> {
        let slot = io.state.actors[usize::from(holder.0)].auras[aura_slot];
        if !slot.active {
            return Ok(()); // stale by content; gen guard normally catches this
        }
        let def = Self::aura_def(io, slot.source, slot.aura)?.clone();
        let Some(tick) = def.tick else { return Ok(()) };

        self.run_range(
            io,
            EffectCtx { source: slot.source, target: holder },
            tick.effect,
        )?;

        // Reschedule the next tick while the aura outlives it. Fixed-period
        // trains for the scaffold; hasted trains re-anchor via gen bump later.
        let next = io.state.now + tick.period;
        if next <= slot.expires_at
            && io.state.actors[usize::from(holder.0)].auras[aura_slot].active
        {
            io.sched.schedule(
                next,
                Priority::PeriodicTick,
                EventPayload::PeriodicTick { holder, slot: aura_slot },
                Some(GenGuard::Aura { holder, slot: aura_slot, gen: slot.gen }),
            );
        }
        Ok(())
    }

    fn aura_expired(
        &self,
        io: &mut EngineIo<'_>,
        holder: ActorId,
        aura_slot: usize,
    ) -> Result<(), EngineError> {
        let had_rate_mult = {
            let slot = io.state.actors[usize::from(holder.0)].auras[aura_slot];
            let def = Self::aura_def(io, slot.source, slot.aura)?;
            def.rate_mult.is_some()
        };
        {
            let slot = &mut io.state.actors[usize::from(holder.0)].auras[aura_slot];
            slot.active = false;
            slot.stacks = 0;
            slot.gen.bump(); // invalidate any remaining ticks
        }
        if had_rate_mult {
            Self::recompute_rates(io, holder)?;
        }
        Ok(())
    }
}
