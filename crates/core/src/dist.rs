//! Declared distributions, collapsed to concrete values under a seed.

use serde::{Deserialize, Serialize};

use crate::ids::Seed;
use crate::rng::{self, Domain};
use crate::time::SimDuration;

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
    /// The smallest and largest values `sample` can return. `lo > hi` means
    /// the declaration is invalid.
    fn bounds(&self) -> (T, T);
}

impl Sample<f64> for Dist<f64> {
    fn sample(&self, seed: Seed, domain: Domain, index: u64) -> f64 {
        let (lo, hi) = self.bounds();
        match self {
            Dist::Fixed(v) => *v,
            Dist::Jitter { .. } | Dist::Uniform { .. } => {
                lo + rng::unit(seed, domain, index) * (hi - lo)
            }
        }
    }

    fn mean(&self) -> f64 {
        let (lo, hi) = self.bounds();
        (lo + hi) / 2.0
    }

    fn bounds(&self) -> (f64, f64) {
        match *self {
            Dist::Fixed(v) => (v, v),
            Dist::Jitter { base, jitter } => (base - jitter, base + jitter),
            Dist::Uniform { lo, hi } => (lo, hi),
        }
    }
}

/// Inclusive of both ends, in whole milliseconds. A jitter wider than its
/// base is clamped at zero.
impl Sample<SimDuration> for Dist<SimDuration> {
    fn sample(&self, seed: Seed, domain: Domain, index: u64) -> SimDuration {
        let (lo, hi) = self.bounds();
        if hi <= lo {
            return lo;
        }
        let width = u64::from(hi.millis() - lo.millis()) + 1;
        let offset = rng::bits(seed, domain, index) % width;
        SimDuration(lo.millis() + offset as u32)
    }

    fn mean(&self) -> SimDuration {
        let (lo, hi) = self.bounds();
        SimDuration(((u64::from(lo.millis()) + u64::from(hi.millis())) / 2) as u32)
    }

    fn bounds(&self) -> (SimDuration, SimDuration) {
        match *self {
            Dist::Fixed(v) => (v, v),
            Dist::Jitter { base, jitter } => (
                SimDuration(base.millis().saturating_sub(jitter.millis())),
                base + jitter,
            ),
            Dist::Uniform { lo, hi } => (lo, hi),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Purpose;

    fn d() -> Domain {
        rng::domain(Purpose::TravelTime, &["test"])
    }

    #[test]
    fn fixed_is_fixed() {
        let dist = Dist::Fixed(3.5);
        assert_eq!(dist.sample(Seed(1), d(), 0), 3.5);
        assert_eq!(dist.mean(), 3.5);
    }

    #[test]
    fn f64_samples_stay_in_bounds() {
        let dist = Dist::Jitter {
            base: 100.0,
            jitter: 10.0,
        };
        for i in 0..1_000 {
            let v = dist.sample(Seed(1), d(), i);
            assert!((90.0..=110.0).contains(&v), "{v}");
        }
        assert_eq!(dist.mean(), 100.0);
    }

    #[test]
    fn durations_cover_both_ends() {
        let dist = Dist::Uniform {
            lo: SimDuration(10),
            hi: SimDuration(12),
        };
        let seen: std::collections::BTreeSet<_> =
            (0..200).map(|i| dist.sample(Seed(9), d(), i)).collect();
        assert_eq!(
            seen.into_iter().collect::<Vec<_>>(),
            vec![SimDuration(10), SimDuration(11), SimDuration(12)]
        );
    }

    #[test]
    fn duration_jitter_clamps_at_zero() {
        let dist = Dist::Jitter {
            base: SimDuration(5),
            jitter: SimDuration(8),
        };
        assert_eq!(dist.bounds(), (SimDuration(0), SimDuration(13)));
    }

    #[test]
    fn same_address_same_value() {
        let dist = Dist::Uniform { lo: 0.0, hi: 1.0 };
        assert_eq!(dist.sample(Seed(4), d(), 7), dist.sample(Seed(4), d(), 7));
    }
}
