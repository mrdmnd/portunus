//! Predicate expression language (SCAFFOLD.md §3).
//!
//! Plain data here; compilation (`compile_predicate -> PredicateId`), analytic
//! crossing solves, and event-subscription rechecks live in `sim-engine`.
//! Language subset: comparisons over resources, cooldown remaining, aura
//! remaining/stacks, and time.

use serde::{Deserialize, Serialize};

use crate::ids::{AuraId, ResourceId, SlotId};

/// Comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmpOp {
    Ge,
    Gt,
    Le,
    Lt,
}

/// A scalar observable of the deciding actor's own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScalarRef {
    /// Current (analytically materialized) value of a resource.
    Resource(ResourceId),
    /// Seconds until an ability slot's cooldown is ready (0 if ready).
    CooldownRemaining(SlotId),
    /// Seconds remaining on a self-aura sourced from this actor (0 if absent).
    AuraRemaining(AuraId),
    /// Stack count of a self-aura sourced from this actor (0 if absent).
    AuraStacks(AuraId),
    /// Seconds since pull start.
    TimeSeconds,
}

/// A predicate expression tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PredicateExpr {
    /// `lhs <op> rhs`.
    Cmp { lhs: ScalarRef, op: CmpOp, rhs: f64 },
    /// True iff all children are true.
    All(Vec<PredicateExpr>),
    /// True iff any child is true.
    Any(Vec<PredicateExpr>),
}
