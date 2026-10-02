//! Counter-based, name-keyed randomness.
//!
//! Every draw is addressed by `(seed, domain, index)`. The domain says who is
//! rolling for what and is built from stable names, never from draw order;
//! the index counts occurrences within that domain. Editing one pull or one
//! spec therefore never shifts another's rolls, which keeps paired
//! comparisons paired across spec edits.

use serde::{Deserialize, Serialize};

use crate::ids::Seed;

/// Who is rolling, for what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Domain(pub u64);

/// What a draw is for. Part of the domain key, so renumbering a variant is a
/// determinism-breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum Purpose {
    TravelTime = 1,
    EnemyHealth = 2,
    EnemyRuleTiming = 3,
    EnemyTarget = 4,
    EnemyDamage = 5,
    Crit = 16,
    Proc = 17,
    /// Streams declared by a spec kit.
    Mechanics = 18,
    /// Per-seat reaction delays.
    Reaction = 19,
    /// Stochastic policies, keyed by decision.
    Policy = 20,
}

/// The one random primitive. Same `(seed, domain, index)`, same bits, on every
/// platform and in any draw order.
pub trait KeyedRng {
    /// Hash stable names (pull, spawn, rule; or actor, stream) into a domain.
    fn domain(&self, purpose: Purpose, names: &[&str]) -> Domain;
    fn bits(&self, seed: Seed, domain: Domain, index: u64) -> u64;
    /// Uniform in `[0, 1)`.
    fn unit(&self, seed: Seed, domain: Domain, index: u64) -> f64;
}
