//! Predicate compilation, evaluation, and wake solving (SCAFFOLD.md §3).
//!
//! Two backends: **analytic crossing-solve** (e.g. `energy >= 80` becomes an
//! exact wake time, re-checked at fire since a rate mutation may have moved
//! the crossing) and **event-subscription + recheck** (conditions that jump
//! discontinuously — lumpy resources, aura state — are re-evaluated after
//! every processed event).

use serde::{Deserialize, Serialize};
use sim_types::{ActorId, CmpOp, CompiledActor, PredicateExpr, PredicateId, ScalarRef, SimTime};

use crate::arena::{ActorKind, SimState};

/// How a compiled predicate can be waited on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakePlan {
    /// Exact analytic crossing at this time.
    At(SimTime),
    /// No analytic solve available: arm a recheck-after-every-event
    /// subscription.
    Subscribe,
}

/// Registry of compiled predicate expressions. Compilation is just interning
/// in the scaffold; the id is stable for the registry's lifetime.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PredicateRegistry {
    exprs: Vec<PredicateExpr>,
}

impl PredicateRegistry {
    pub fn compile(&mut self, expr: &PredicateExpr) -> PredicateId {
        if let Some(i) = self.exprs.iter().position(|e| e == expr) {
            return PredicateId(i as u16);
        }
        self.exprs.push(expr.clone());
        PredicateId((self.exprs.len() - 1) as u16)
    }

    #[must_use]
    pub fn get(&self, id: PredicateId) -> Option<&PredicateExpr> {
        self.exprs.get(usize::from(id.0))
    }
}

fn scalar_value(state: &SimState, actor: ActorId, s: ScalarRef) -> f64 {
    let a = &state.actors[usize::from(actor.0)];
    let now = state.now;
    match s {
        ScalarRef::Resource(r) => {
            a.resources.get(usize::from(r.0)).map_or(0.0, |res| res.value_at(now))
        }
        ScalarRef::CooldownRemaining(slot) => {
            let ready = a.cooldown_ready.get(usize::from(slot.0)).copied().unwrap_or(0);
            sim_types::to_secs_f32(ready.saturating_sub(now)).into()
        }
        ScalarRef::AuraRemaining(aura) => a.find_aura(aura, actor).map_or(0.0, |i| {
            sim_types::to_secs_f32(a.auras[i].expires_at.saturating_sub(now)).into()
        }),
        ScalarRef::AuraStacks(aura) => {
            a.find_aura(aura, actor).map_or(0.0, |i| f64::from(a.auras[i].stacks))
        }
        ScalarRef::TimeSeconds => now as f64 / sim_types::MICROS_PER_SEC as f64,
    }
}

fn cmp(op: CmpOp, lhs: f64, rhs: f64) -> bool {
    match op {
        CmpOp::Ge => lhs >= rhs,
        CmpOp::Gt => lhs > rhs,
        CmpOp::Le => lhs <= rhs,
        CmpOp::Lt => lhs < rhs,
    }
}

/// Evaluate a predicate against current state.
#[must_use]
pub fn eval(state: &SimState, actor: ActorId, expr: &PredicateExpr) -> bool {
    match expr {
        PredicateExpr::Cmp { lhs, op, rhs } => cmp(*op, scalar_value(state, actor, *lhs), *rhs),
        PredicateExpr::All(children) => children.iter().all(|c| eval(state, actor, c)),
        PredicateExpr::Any(children) => children.iter().any(|c| eval(state, actor, c)),
    }
}

/// Plan a wake for a predicate that is currently false.
///
/// Analytic solves are available for upward resource comparisons (steady
/// regen) and time comparisons; everything else subscribes. Conjunctions and
/// disjunctions subscribe unless trivially reducible — exactness over
/// cleverness in the scaffold.
#[must_use]
pub fn plan_wake(
    state: &SimState,
    actor: ActorId,
    roster: &[CompiledActor],
    expr: &PredicateExpr,
) -> WakePlan {
    match expr {
        PredicateExpr::Cmp { lhs: ScalarRef::Resource(r), op: CmpOp::Ge | CmpOp::Gt, rhs } => {
            let a = &state.actors[usize::from(actor.0)];
            debug_assert!(matches!(a.kind, ActorKind::Player { .. }));
            match a.resources.get(usize::from(r.0)).and_then(|res| {
                // Strict `>` needs an epsilon past the crossing; one microsecond
                // of regen is below any meaningful threshold granularity.
                res.time_to_reach(state.now, *rhs)
            }) {
                Some(t) => WakePlan::At(t.max(state.now)),
                None => WakePlan::Subscribe,
            }
        }
        PredicateExpr::Cmp { lhs: ScalarRef::TimeSeconds, op: CmpOp::Ge | CmpOp::Gt, rhs } => {
            WakePlan::At(sim_types::from_secs_f64(rhs.max(0.0)).max(state.now))
        }
        PredicateExpr::Cmp { lhs: ScalarRef::CooldownRemaining(slot), op: CmpOp::Le | CmpOp::Lt, rhs } => {
            let a = &state.actors[usize::from(actor.0)];
            let ready = a.cooldown_ready.get(usize::from(slot.0)).copied().unwrap_or(0);
            let lead = sim_types::from_secs_f64(rhs.max(0.0));
            WakePlan::At(ready.saturating_sub(lead).max(state.now))
        }
        _ => {
            let _ = roster;
            WakePlan::Subscribe
        }
    }
}
