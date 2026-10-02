//! Experiments.
//!
//! An [`Arm`] is anything that turns a seed into an [`Outcome`]: a fixed
//! combination of scenario, loadouts, plan, and policies. Every arm in an
//! experiment runs on the same seeds, so differences between arms are
//! measured pairwise, which cancels most of the run-to-run noise.

use std::sync::Arc;

use portunus_core::{Seat, Seed};
use portunus_engine::Outcome;
use thiserror::Error;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    TotalTime,
    CompletionRate,
    Deaths,
    SeatDamage(Seat),
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
    /// Each non-baseline arm minus the baseline, per metric, on paired seeds.
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
