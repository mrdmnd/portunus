//! Where episodes come from.

use std::sync::Arc;

use portunus_core::Seed;
use portunus_engine::RunSetup;
use portunus_plan::Plan;
use portunus_scenario::TimelinePriors;

#[derive(Debug, Clone)]
pub struct Episode {
    pub setup: RunSetup,
    pub plan: Arc<Plan>,
    pub priors: Arc<TimelinePriors>,
}

/// Binds a scenario, a set of loadouts, and a plan; samples per seed.
pub trait EpisodeSource: Send + Sync {
    fn episode(&self, seed: Seed) -> Episode;
}
