//! Declared distributions, collapsed to concrete values under a seed.

use serde::{Deserialize, Serialize};

use crate::ids::Seed;
use crate::rng::Domain;

/// A value an author declares as uncertain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dist<T> {
    Fixed(T),
    /// Uniform on `[base - jitter, base + jitter]`.
    Jitter {
        base: T,
        jitter: T,
    },
    /// Uniform on `[lo, hi]`.
    Uniform {
        lo: T,
        hi: T,
    },
}

pub trait Sample<T> {
    fn sample(&self, seed: Seed, domain: Domain, index: u64) -> T;
    /// The value a realistic player would plan around.
    fn mean(&self) -> T;
}
