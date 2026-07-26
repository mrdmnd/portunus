//! Integer simulation time (invariant 1: no float time, no wall clock).

/// Microseconds since pull start.
pub type SimTime = u64;

/// One second in [`SimTime`] units.
pub const MICROS_PER_SEC: SimTime = 1_000_000;

/// One millisecond in [`SimTime`] units.
pub const MICROS_PER_MILLI: SimTime = 1_000;

/// Convert milliseconds to [`SimTime`].
#[must_use]
pub const fn from_millis(ms: u64) -> SimTime {
    ms * MICROS_PER_MILLI
}

/// Convert (non-negative, finite) seconds to [`SimTime`], rounding up so a
/// computed wake time never lands *before* the crossing it names.
#[must_use]
pub fn from_secs_f64(secs: f64) -> SimTime {
    debug_assert!(secs >= 0.0 && secs.is_finite());
    (secs * MICROS_PER_SEC as f64).ceil() as SimTime
}

/// Convert a [`SimTime`] span to `f32` seconds (for observation-facing fields
/// such as `ActionMask::usable_in`; never used in semantic time paths).
#[must_use]
pub fn to_secs_f32(t: SimTime) -> f32 {
    t as f32 / MICROS_PER_SEC as f32
}
