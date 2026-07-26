//! The effect-IR vocabulary: enum dispatch on the hot path, no `dyn`.
//!
//! Grown additively as tier-3 mechanics demand; each variant must pass the
//! decision-point completeness review (§3) — its decision-relevance may only
//! change via events, crossings, or declarable predicates. The full design
//! vocabulary (~30–40 primitives) grows from this seed; `Scripted` escape
//! hatches are deliberately absent from the scaffold (the toy codex must be
//! expressible in pure IR, invariant 2's test).

use serde::{Deserialize, Serialize};
use sim_types::{AuraId, EffectRange, ResourceId, StreamId};

/// One effect primitive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EffectOp {
    /// Deal a flat damage lump to the effect's target.
    /// (School/coefficient/AoE-cap parameters land with `sim-codex`.)
    DealDamage { amount: f64 },
    /// Apply (or refresh) an aura from the source's kit, on self or target.
    ApplyAura { aura: AuraId, on_self: bool },
    /// Grant a resource lump to the source.
    GrantResource { resource: ResourceId, amount: f64 },
    /// Burn a resource lump from the source (costs are kernel gates; this is
    /// for effects that drain beyond the cost).
    SpendResource { resource: ResourceId, amount: f64 },
    /// Roll a proc on a dedicated semantic RNG stream (invariant 6) and, on
    /// success, run a nested op range. (RPPM/ICD/BLP land with `sim-codex`.)
    TriggerProc { chance: f64, stream: StreamId, effect: EffectRange },
}

/// The flat op arena all `EffectRange`s index into. Built by the codex
/// compiler (today: the toy loader), immutable during a rollout.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OpArena {
    pub ops: Vec<EffectOp>,
}

impl OpArena {
    /// Resolve a range to its ops. Empty on out-of-bounds (compiler bug).
    #[must_use]
    pub fn range(&self, r: EffectRange) -> &[EffectOp] {
        let start = r.start as usize;
        let end = start + r.len as usize;
        self.ops.get(start..end).unwrap_or(&[])
    }

    /// Append a compiled op list, returning its range. Callers must append
    /// nested (child) ranges *before* the parent list so every range stays
    /// contiguous.
    pub fn push_list(&mut self, ops: impl IntoIterator<Item = EffectOp>) -> EffectRange {
        let start = self.ops.len() as u32;
        self.ops.extend(ops);
        EffectRange { start, len: self.ops.len() as u32 - start }
    }
}
