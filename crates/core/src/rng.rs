//! Counter-based, name-keyed randomness.
//!
//! Every draw is addressed by `(seed, domain, index)`. The domain says who is
//! rolling for what and is built from stable names, never from draw order;
//! the index counts occurrences within that domain. Editing one pull or one
//! spec therefore never shifts another's rolls, which keeps paired
//! comparisons paired across spec edits.
//!
//! There is exactly one implementation, using only integer operations, so
//! the same address yields the same bits on every platform. Changing any
//! constant or step here changes every rollout.

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
    EnemyRuleTiming = 3,
    EnemyTarget = 4,
    EnemyDamage = 5,
    /// Per-pull enemy health multipliers.
    EnemyHealth = 6,
    Crit = 16,
    Proc = 17,
    /// Streams declared by a spec kit.
    Mechanics = 18,
    /// Per-seat reaction delays.
    Reaction = 19,
    /// Stochastic policies, keyed by decision.
    Policy = 20,
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const GOLDEN: u64 = 0x9e37_79b9_7f4a_7c15;

fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// SplitMix64's finalizer.
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Hash stable names (pull, spawn, rule; or actor, stream) into a domain.
/// Each name is length-prefixed, so `["ab", "c"]` and `["a", "bc"]` differ.
pub fn domain(purpose: Purpose, names: &[&str]) -> Domain {
    let mut hash = fnv(FNV_OFFSET, &(purpose as u16).to_le_bytes());
    for name in names {
        hash = fnv(hash, &(name.len() as u64).to_le_bytes());
        hash = fnv(hash, name.as_bytes());
    }
    Domain(mix(hash))
}

pub fn bits(seed: Seed, domain: Domain, index: u64) -> u64 {
    let keyed = mix(seed.0.wrapping_add(GOLDEN)) ^ domain.0;
    mix(mix(keyed).wrapping_add(index.wrapping_mul(GOLDEN)))
}

/// Uniform in `[0, 1)`, from the top 53 bits.
pub fn unit(seed: Seed, domain: Domain, index: u64) -> f64 {
    (bits(seed, domain, index) >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_length_prefixed() {
        assert_ne!(
            domain(Purpose::Crit, &["ab", "c"]),
            domain(Purpose::Crit, &["a", "bc"])
        );
    }

    #[test]
    fn purpose_is_part_of_the_domain() {
        assert_ne!(
            domain(Purpose::Crit, &["seat0"]),
            domain(Purpose::Proc, &["seat0"])
        );
    }

    #[test]
    fn draws_depend_on_every_part_of_the_address() {
        let d = domain(Purpose::Crit, &["seat0"]);
        let base = bits(Seed(1), d, 0);
        assert_ne!(base, bits(Seed(2), d, 0));
        assert_ne!(base, bits(Seed(1), d, 1));
        assert_ne!(base, bits(Seed(1), domain(Purpose::Crit, &["seat1"]), 0));
    }

    #[test]
    fn unit_is_in_range() {
        let d = domain(Purpose::Proc, &["x"]);
        for i in 0..10_000 {
            let u = unit(Seed(7), d, i);
            assert!((0.0..1.0).contains(&u));
        }
    }

    #[test]
    fn unit_mean_is_near_half() {
        let d = domain(Purpose::Proc, &["x"]);
        let n = 100_000;
        let mean = (0..n).map(|i| unit(Seed(3), d, i)).sum::<f64>() / n as f64;
        assert!((mean - 0.5).abs() < 0.005, "mean {mean}");
    }

    /// Pins the algorithm: if this changes, every recorded rollout changes.
    #[test]
    fn golden_values() {
        let d = domain(Purpose::Crit, &["pull", "grunt#1"]);
        let got = [d.0, bits(Seed(42), d, 0), bits(Seed(42), d, 1)];
        assert_eq!(got, GOLDEN_VALUES);
    }

    const GOLDEN_VALUES: [u64; 3] = [
        15_853_651_498_902_783_105,
        13_020_674_950_139_082_553,
        1_441_559_901_521_377_338,
    ];
}
