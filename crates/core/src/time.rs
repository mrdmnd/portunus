//! Integer simulation time, in milliseconds.
//!
//! Instants, spans, and signed offsets are distinct types so one can't be
//! passed for another. Arithmetic overflow is a bug and panics in debug
//! builds.

use std::ops::{Add, AddAssign, Sub};

use serde::{Deserialize, Serialize};

/// Milliseconds since the start of the run.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SimTime(pub u32);

/// A non-negative span of simulation time.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SimDuration(pub u32);

/// A signed span, for times relative to an anchor (negative is before it).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SimOffset(pub i32);

impl SimTime {
    pub const ZERO: Self = Self(0);

    pub const fn millis(self) -> u32 {
        self.0
    }

    /// The span since `earlier`, or zero if `earlier` is later.
    pub const fn saturating_since(self, earlier: Self) -> SimDuration {
        SimDuration(self.0.saturating_sub(earlier.0))
    }
}

impl SimDuration {
    pub const ZERO: Self = Self(0);

    pub const fn from_millis(ms: u32) -> Self {
        Self(ms)
    }

    pub const fn from_secs(secs: u32) -> Self {
        Self(secs * 1_000)
    }

    pub const fn millis(self) -> u32 {
        self.0
    }
}

impl Add<SimDuration> for SimTime {
    type Output = SimTime;
    fn add(self, rhs: SimDuration) -> SimTime {
        SimTime(self.0 + rhs.0)
    }
}

impl AddAssign<SimDuration> for SimTime {
    fn add_assign(&mut self, rhs: SimDuration) {
        self.0 += rhs.0;
    }
}

impl Sub<SimDuration> for SimTime {
    type Output = SimTime;
    fn sub(self, rhs: SimDuration) -> SimTime {
        SimTime(self.0 - rhs.0)
    }
}

impl Sub for SimTime {
    type Output = SimDuration;
    fn sub(self, rhs: SimTime) -> SimDuration {
        SimDuration(self.0 - rhs.0)
    }
}

impl Add for SimDuration {
    type Output = SimDuration;
    fn add(self, rhs: SimDuration) -> SimDuration {
        SimDuration(self.0 + rhs.0)
    }
}

impl Sub for SimDuration {
    type Output = SimDuration;
    fn sub(self, rhs: SimDuration) -> SimDuration {
        SimDuration(self.0 - rhs.0)
    }
}
