//! The HAL vocabulary layer (SCAFFOLD.md §3b).
//!
//! Game semantics implemented as **effect primitives** against the kernel,
//! driven by codex data. Abilities are compositions of [`EffectOp`]s — flat
//! op lists in one arena, not code. Primitives are stateless pure functions;
//! all persistence lives in `SimState` (statelessness is an invariant, not a
//! style). The kernel never depends on this crate.
//!
//! Tier-3 procedure for new mechanics: new `EffectOp` variant + `apply` arm,
//! kernel untouched, and a decision-point completeness review (§3).

pub mod apply;
pub mod ops;
pub mod toy;

pub use apply::IrMechanics;
pub use ops::{EffectOp, OpArena};
