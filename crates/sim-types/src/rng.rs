//! Counter-based deterministic RNG (invariant 6).
//!
//! Philox2x64-10: a small, dependency-free counter-based PRNG. Draws are keyed
//! on *semantic streams*, never on draw order: `draw(seed, domain, occurrence)`
//! where `domain` identifies who/what is rolling (actor + stream, or an
//! encounter-sampling purpose) and `occurrence` is the per-domain counter.
//! This is what makes common-random-number paired comparisons work.
//!
//! This module lives in `sim-types` (rather than the kernel) because both the
//! engine and the encounter sampler need the same deterministic primitive, and
//! the sampler must not depend on the kernel.

use crate::ids::{ActorId, Seed, StreamId};

const PHILOX_M: u64 = 0xD2B7_4407_B1CE_6E93;
const PHILOX_W: u64 = 0x9E37_79B9_7F4A_7C15; // golden-ratio Weyl key bump

#[inline]
fn mulhilo(a: u64, b: u64) -> (u64, u64) {
    let p = u128::from(a) * u128::from(b);
    ((p >> 64) as u64, p as u64)
}

/// One Philox2x64 bijection: 10 rounds over a 128-bit counter with a 64-bit key.
#[must_use]
pub fn philox2x64(counter: [u64; 2], key: u64) -> [u64; 2] {
    let (mut x0, mut x1) = (counter[0], counter[1]);
    let mut k = key;
    for _ in 0..10 {
        let (hi, lo) = mulhilo(PHILOX_M, x0);
        x0 = hi ^ k ^ x1;
        x1 = lo;
        k = k.wrapping_add(PHILOX_W);
    }
    [x0, x1]
}

/// Draw a `u64` for `(seed, domain, occurrence)`. Same triple, same result —
/// on every platform, in any draw order.
#[must_use]
pub fn draw_u64(seed: Seed, domain: u64, occurrence: u64) -> u64 {
    philox2x64([occurrence, domain], seed.0)[0]
}

/// Domain key for an actor-scoped semantic stream (proc rolls, crit rolls, ...).
#[must_use]
pub fn actor_stream_domain(actor: ActorId, stream: StreamId) -> u64 {
    // Distinct from encounter-sampling domains via the high tag byte.
    (1_u64 << 56) | (u64::from(actor.0) << 16) | u64::from(stream.0)
}

/// Domain key for an encounter-sampling purpose (mob HP, event jitter, ...).
/// `purpose` is a small enum-like tag defined by the sampler.
#[must_use]
pub fn encounter_domain(purpose: u16, index: u32) -> u64 {
    (2_u64 << 56) | (u64::from(purpose) << 32) | u64::from(index)
}

/// Map a draw to a uniform in `[0, 1)` using the top 53 bits.
#[must_use]
pub fn uniform01(x: u64) -> f64 {
    (x >> 11) as f64 * (1.0 / (1_u64 << 53) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_stream_independent() {
        let s = Seed(42);
        let a = draw_u64(s, actor_stream_domain(ActorId(0), StreamId(1)), 0);
        let b = draw_u64(s, actor_stream_domain(ActorId(0), StreamId(1)), 0);
        assert_eq!(a, b);
        // Different occurrence, stream, actor, or seed all decorrelate.
        assert_ne!(a, draw_u64(s, actor_stream_domain(ActorId(0), StreamId(1)), 1));
        assert_ne!(a, draw_u64(s, actor_stream_domain(ActorId(0), StreamId(2)), 0));
        assert_ne!(a, draw_u64(s, actor_stream_domain(ActorId(1), StreamId(1)), 0));
        assert_ne!(a, draw_u64(Seed(43), actor_stream_domain(ActorId(0), StreamId(1)), 0));
    }

    #[test]
    fn uniform01_in_range() {
        for i in 0..1000 {
            let u = uniform01(draw_u64(Seed(7), encounter_domain(0, i), 0));
            assert!((0.0..1.0).contains(&u));
        }
    }
}
