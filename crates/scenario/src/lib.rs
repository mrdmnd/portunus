//! Scenarios: what you pull, in what order, and when each enemy joins.
//!
//! - [`spec`]: the authored route. References enemies by key only; what they
//!   do lives in [`portunus_gamedata::EnemyData`].
//! - [`resolved`]: one concrete run, with every *static* random value drawn
//!   (travel times). Dynamic values (rule timings, targets)
//!   are drawn by the engine under the same seed, in domains keyed by pull
//!   and spawn label, so rollouts stay reproducible and paired.
//! - [`ScenarioSampler`]: validates once, then samples per seed;
//!   [`Sampler`] is the implementation.

pub mod resolved;
mod sampler;
pub mod spec;

use std::sync::Arc;

use portunus_core::{EnemyKey, EventName, PullName, Seed, SimDuration, SpawnLabel};
use portunus_gamedata::EnemyData;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use resolved::ResolvedRun;
pub use sampler::Sampler;
pub use spec::ScenarioSpec;

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("invalid scenario: {0:?}")]
    Invalid(Vec<ScenarioIssue>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioIssue {
    DuplicatePull(PullName),
    EmptyPull(PullName),
    UnknownEnemy {
        pull: PullName,
        enemy: EnemyKey,
    },
    /// An engagement trigger names a spawn that isn't in its pull.
    UnknownSpawn {
        pull: PullName,
        spawn: SpawnLabel,
    },
    /// An engagement trigger names a wave index past the pull's last.
    UnknownWave {
        pull: PullName,
        wave: usize,
    },
    /// An engagement trigger names a rule the spawn's enemy doesn't have.
    UnknownEvent {
        pull: PullName,
        spawn: SpawnLabel,
        event: EventName,
    },
    /// No wave can ever engage (e.g. it waits on itself).
    NeverEngages {
        pull: PullName,
        wave: usize,
    },
    /// The pre-pull window can be longer than the shortest travel time.
    PrepullExceedsTravel(PullName),
    /// The travel time's bounds are reversed.
    InvalidTravel(PullName),
    /// A wave's distance is negative or not a number.
    InvalidDistance {
        pull: PullName,
        wave: usize,
    },
    NotEnoughForces {
        required: u32,
        available: u32,
    },
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
