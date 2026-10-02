//! Scenarios: what you pull, in what order, and when each enemy joins.
//!
//! - [`spec`]: the authored route. References enemies by key only; what they
//!   do lives in [`portunus_gamedata::EnemyData`].
//! - [`resolved`]: one concrete run, with every *static* random value drawn
//!   (travel times, enemy health). Dynamic values (rule timings, targets)
//!   are drawn by the engine from the same name-keyed random stream, via
//!   each spawn's domain, so rollouts stay reproducible and paired.
//! - [`ScenarioSampler`]: validates once, then samples per seed.

pub mod resolved;
pub mod spec;

use std::sync::Arc;

use portunus_core::{EventName, PullName, Seed, SimDuration, SpawnLabel};
use portunus_gamedata::EnemyData;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use resolved::ResolvedRun;
pub use spec::ScenarioSpec;

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("invalid scenario:\n  {}", .0.join("\n  "))]
    Invalid(Vec<String>),
}

/// What a realistic player knows about enemy timing in advance: averages,
/// not the sampled truth.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TimelinePriors {
    pub events: Vec<EventPrior>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventPrior {
    pub pull: PullName,
    pub spawn: SpawnLabel,
    pub event: EventName,
    /// Mean time from the spawn's engagement to the first firing, if timed.
    pub first_mean: Option<SimDuration>,
    pub interval_mean: Option<SimDuration>,
}

pub trait ScenarioSampler: Sized {
    /// Validate against the enemy data; report every problem.
    fn new(spec: ScenarioSpec, enemies: Arc<EnemyData>) -> Result<Self, ScenarioError>;
    fn spec(&self) -> &ScenarioSpec;
    /// Same seed, same run.
    fn sample(&self, seed: Seed) -> ResolvedRun;
    fn priors(&self) -> TimelinePriors;
}
