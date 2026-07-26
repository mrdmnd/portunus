//! The simulation kernel (SCAFFOLD.md §3).
//!
//! This crate contains **zero game-specific knowledge** (invariant 2). It
//! implements only abstract machinery: the event queue with deterministic
//! `(time, priority, seq)` ordering, generation-counter lazy invalidation,
//! analytic `(value, t0, rate)` resources, predicate compilation and wake
//! solving, and the decision-point loop. Game semantics plug in through the
//! [`Mechanics`] trait, implemented by `sim-mechanics` against the effect IR.
//!
//! Determinism contract (invariant 1): given (roster, resolved run, plan,
//! seed) and a deterministic policy, a rollout is bit-identical across runs
//! and platforms. All time is integer microseconds; nothing here reads
//! a clock or iterates a `HashMap`.

pub mod arena;
pub mod engine;
pub mod error;
pub mod event;
pub mod predicate;
pub mod priority;
pub mod resources;
pub mod scheduler;

pub use arena::{ActorKind, ActorState, AuraSlot, SimState};
pub use engine::{
    DecisionPoint, Engine, EngineIo, Mechanics, NoopMechanics, ReferenceEngine, RolloutContext,
    Snapshot, Step, WakeReason,
};
pub use error::EngineError;
pub use event::{EventPayload, GenGuard};
pub use priority::Priority;
pub use resources::AnalyticResource;
pub use scheduler::EventScheduler;
