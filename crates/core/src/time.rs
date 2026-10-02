//! Integer simulation time.

/// Milliseconds since the start of the run.
pub type SimTime = u32;

/// A span of simulation time, in milliseconds.
pub type SimDuration = u32;

pub const MILLIS_PER_SEC: SimDuration = 1_000;
