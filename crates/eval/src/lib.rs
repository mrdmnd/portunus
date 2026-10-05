//! Experiments.
//!
//! An [`Arm`] is anything that turns a seed into an [`Outcome`]: a fixed
//! combination of scenario, loadouts, plan, and policies. Every arm in an
//! experiment runs on the same seeds, so differences between arms are
//! measured pairwise, which cancels most of the run-to-run noise.
//!
//! [`LocalRunner`] runs experiments on this machine's threads.

mod runner;

use std::sync::Arc;

use portunus_core::{PullName, Seat, Seed};
use portunus_engine::Outcome;
use thiserror::Error;

pub use runner::{estimate, metric_value, LocalRunner};

#[derive(Debug, Error)]
pub enum ArmError {
    #[error("rollout failed on seed {seed:?}: {message}")]
    Rollout { seed: Seed, message: String },
}

pub trait Arm: Send + Sync {
    fn name(&self) -> &str;
    fn rollout(&self, seed: Seed) -> Result<Outcome, ArmError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedSet {
    pub first: u64,
    pub count: u32,
}

impl SeedSet {
    pub fn iter(&self) -> impl Iterator<Item = Seed> {
        (self.first..self.first + u64::from(self.count)).map(Seed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Metric {
    TotalTime,
    CompletionRate,
    Deaths,
    SeatDamage(Seat),
    /// Damage per second in combat: time from each pull's start to its clear
    /// (or the end of the run).
    SeatDps(Seat),
    /// 1 if anyone died in this pull, else 0; its mean is the pull's death
    /// chance.
    DeathChance(PullName),
}

/// One value per seed: `values[i]` is for seed `seeds.first + i`, and `None`
/// marks a failed rollout. Results on the same seeds pair up index by index;
/// a seed that failed on either side is left out of the pair.
#[derive(Debug, Clone, PartialEq)]
pub struct PerSeed {
    pub seeds: SeedSet,
    pub values: Vec<Option<f64>>,
}

pub struct Experiment {
    pub arms: Vec<Arc<dyn Arm>>,
    /// Index of the arm others are compared against.
    pub baseline: usize,
    pub seeds: SeedSet,
    pub metrics: Vec<Metric>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub n: u32,
    pub mean: f64,
    pub std_err: f64,
    pub ci95: (f64, f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub arms: Vec<ArmReport>,
    /// Each non-baseline arm minus the baseline, per metric, on the seeds
    /// both completed.
    pub paired: Vec<PairedDelta>,
    pub failures: Vec<(String, Seed)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArmReport {
    pub name: String,
    pub metrics: Vec<(Metric, Estimate)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PairedDelta {
    pub arm: String,
    pub metric: Metric,
    pub delta: Estimate,
}

pub trait Runner {
    fn run(&self, experiment: &Experiment) -> Report;
}
