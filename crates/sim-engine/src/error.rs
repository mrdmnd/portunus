//! Kernel error type.

use sim_types::{ActorId, PredicateId, SlotId};
use thiserror::Error;

/// Errors surfaced by the kernel. Policy-facing misuse (illegal actions,
/// livelock) is an error, not a panic, so optimizers can treat bad candidate
/// policies as failed rollouts rather than crashes.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EngineError {
    #[error("engine has not been reset with a rollout context")]
    NotReset,

    #[error("no decision is pending for actor {0:?}")]
    NoPendingDecision(ActorId),

    #[error("decision pending for actor {expected:?}, but {got:?} submitted")]
    WrongActor { expected: ActorId, got: ActorId },

    #[error("illegal action by {actor:?}: {reason}")]
    IllegalAction { actor: ActorId, reason: String },

    #[error("livelock: {0:?} submitted bare Wait(NextEvent) twice on the same trigger")]
    Livelock(ActorId),

    #[error("unknown predicate {0:?}")]
    UnknownPredicate(PredicateId),

    #[error("unknown ability slot {slot:?} for actor {actor:?}")]
    UnknownSlot { actor: ActorId, slot: SlotId },

    #[error("mechanics error: {0}")]
    Mechanics(String),
}
