//! Analytic resources (invariant 3).
//!
//! Between events, state evolves analytically: a resource is a
//! `(value_at_t0, t0, rate)` triple, materialized on demand and re-anchored
//! on every mutation. Skip-to-next-event is exact, not approximate.

use serde::{Deserialize, Serialize};
use sim_types::{SimTime, MICROS_PER_SEC};

/// A linearly evolving, clamped scalar resource.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AnalyticResource {
    value_at_t0: f64,
    t0: SimTime,
    rate_per_sec: f64,
    min: f64,
    max: f64,
}

impl AnalyticResource {
    #[must_use]
    pub fn new(initial: f64, t0: SimTime, rate_per_sec: f64, min: f64, max: f64) -> Self {
        debug_assert!(min <= max);
        Self { value_at_t0: initial.clamp(min, max), t0, rate_per_sec, min, max }
    }

    /// Materialize the value at `t >= t0`.
    #[must_use]
    pub fn value_at(&self, t: SimTime) -> f64 {
        debug_assert!(t >= self.t0, "materialization before anchor");
        let dt = (t - self.t0) as f64 / MICROS_PER_SEC as f64;
        (self.value_at_t0 + self.rate_per_sec * dt).clamp(self.min, self.max)
    }

    /// Current regeneration rate per second.
    #[must_use]
    pub fn rate(&self) -> f64 {
        self.rate_per_sec
    }

    #[must_use]
    pub fn max(&self) -> f64 {
        self.max
    }

    /// Add a lump `delta` at time `t`, re-anchoring the triple.
    pub fn add(&mut self, t: SimTime, delta: f64) {
        self.value_at_t0 = (self.value_at(t) + delta).clamp(self.min, self.max);
        self.t0 = t;
    }

    /// Change the rate at time `t`, re-anchoring the triple (this is the
    /// "haste mutation re-anchors analytic triples" path).
    pub fn set_rate(&mut self, t: SimTime, rate_per_sec: f64) {
        self.value_at_t0 = self.value_at(t);
        self.t0 = t;
        self.rate_per_sec = rate_per_sec;
    }

    /// Exact earliest time `>= now` at which the value reaches `threshold`,
    /// or `None` if unreachable under the current rate and clamps.
    /// This is the analytic crossing solve behind predicate wakes and
    /// mask-change auto-scheduling.
    #[must_use]
    pub fn time_to_reach(&self, now: SimTime, threshold: f64) -> Option<SimTime> {
        let v = self.value_at(now);
        if v >= threshold {
            return Some(now);
        }
        if self.rate_per_sec <= 0.0 || threshold > self.max {
            return None;
        }
        let secs = (threshold - v) / self.rate_per_sec;
        Some(now + sim_types::from_secs_f64(secs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::{rng, Seed};

    /// G0 property test: analytic materialization is exact at random probe
    /// times after a random walk of lumps and rate changes.
    #[test]
    fn materialization_exact_under_random_walk() {
        let mut r = AnalyticResource::new(50.0, 0, 10.0, 0.0, 100.0);
        let mut expected = 50.0_f64;
        let mut last_t: SimTime = 0;
        let mut rate = 10.0_f64;

        for i in 0..500_u32 {
            let d0 = rng::draw_u64(Seed(9), rng::encounter_domain(0, i), 0);
            let d1 = rng::draw_u64(Seed(9), rng::encounter_domain(1, i), 0);
            let t = last_t + 1 + d0 % 3_000_000;

            // Manually integrate the expected value up to t.
            let dt = (t - last_t) as f64 / MICROS_PER_SEC as f64;
            expected = (expected + rate * dt).clamp(0.0, 100.0);
            last_t = t;

            match d1 % 3 {
                0 => {
                    let lump = rng::uniform01(d1) * 60.0 - 30.0;
                    r.add(t, lump);
                    expected = (expected + lump).clamp(0.0, 100.0);
                }
                1 => {
                    rate = rng::uniform01(d1) * 20.0;
                    r.set_rate(t, rate);
                }
                _ => {
                    assert!(
                        (r.value_at(t) - expected).abs() < 1e-6,
                        "probe at t={t}: got {}, expected {expected}",
                        r.value_at(t)
                    );
                }
            }
        }
    }

    #[test]
    fn crossing_solve_is_exact_and_conservative() {
        let r = AnalyticResource::new(20.0, 0, 10.0, 0.0, 100.0);
        let t = r.time_to_reach(0, 80.0).unwrap();
        // 60 units at 10/s = 6s.
        assert_eq!(t, 6 * MICROS_PER_SEC);
        assert!(r.value_at(t) >= 80.0 - 1e-9);
        // Already there.
        assert_eq!(r.time_to_reach(0, 10.0), Some(0));
        // Unreachable: above max.
        assert_eq!(r.time_to_reach(0, 101.0), None);
        // Unreachable: no regen.
        let dead = AnalyticResource::new(0.0, 0, 0.0, 0.0, 100.0);
        assert_eq!(dead.time_to_reach(0, 1.0), None);
    }
}
