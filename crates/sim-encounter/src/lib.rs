//! The run definition layer: the "pull sheet" DSL and its compiler.
//!
//! A run file describes *what you pull and when* — nothing about what mobs
//! do (that is codex data, referenced by name from the bestiary):
//!
//! - **pulls**, in order, each with the mobs entering that combat;
//! - **engagement** per wave: the opening gather, timed body-pulls
//!   (`after_ms`), or chain conditions (`when`) over combat state — pulling
//!   trash onto a boss is just several waves in one pull;
//! - **travel time** between pulls, as an explicit distribution;
//! - **static cooldown assignments** per pull, compiled into `Plan` windows
//!   that policies read (never forced casts).
//!
//! Compilation: `(RunSpec, Bestiary)` validates once, then
//! [`ScenarioSampler::sample`] collapses every declared distribution —
//! travel jitter, HP rolls, script timing, intake targets — into a concrete
//! [`ResolvedRun`] under a seed. RNG domains are keyed on *name hashes*
//! (pull/spawn/script), so editing one pull does not perturb another pull's
//! rolls and common-random-number pairing survives spec edits.

pub mod bestiary;
pub mod compile;
pub mod run;

pub use compile::{RunError, RunSampler};
pub use run::RunSpec;

use sim_types::{ResolvedRun, Seed, TimelinePriors};

/// Distributions in, concrete run out (same seed, same run — bit-identical,
/// per invariant 1).
pub trait ScenarioSampler {
    fn sample(&self, seed: Seed) -> ResolvedRun;
    /// What a "realistic" InfoSet substitutes for privileged timeline truth.
    fn priors(&self) -> TimelinePriors;
}
