//! Declared distribution types.
//!
//! Anywhere the world is stochastic, the authoring surface says so with one
//! of these — never a bare number that secretly gets fuzzed. All randomness
//! is spent at sample time on semantic RNG streams; nothing downstream of a
//! resolved artifact draws from these again.

use serde::{Deserialize, Serialize};

/// A duration distribution: `base ± jitter`, uniform, integer milliseconds.
/// Integer arithmetic end to end — no float in any sampled time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JitteredMs {
    pub base: u64,
    #[serde(default)]
    pub jitter: u64,
}

impl JitteredMs {
    /// A fixed (jitterless) duration.
    #[must_use]
    pub const fn fixed(base: u64) -> Self {
        Self { base, jitter: 0 }
    }
}

/// A health distribution: `mean * (1 ± jitter_frac)`, uniform.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HpSpec {
    pub mean: f64,
    #[serde(default)]
    pub jitter_frac: f64,
}
