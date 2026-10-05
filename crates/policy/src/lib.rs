//! In-combat control.
//!
//! Two kinds of controller, split by what they can touch:
//!
//! - [`Policy`]: sees only its observation. Deployable — hand-written
//!   rules, the SimulationCraft-style baseline, a network on its own, or a
//!   distilled student can all run in an addon.
//! - [`SearchPolicy`]: also gets the env, so it can fork and play ahead.
//!   Used for training targets and as an upper bound; never deployable.
//!
//! A party is just a policy that dispatches on [`Decision::seat`].
//!
//! Policies are stateless: a choice is a function of the decision alone
//! (observation, mask, and [`portunus_env::DecisionKey`] for randomness).
//! Anything a policy wants to "remember" — a burst sequence, a commitment
//! to a plan — must exist in the world state as auras, cooldowns, or plan
//! windows.

use std::sync::Arc;

use portunus_core::{Seat, SimDuration};
use portunus_engine::Choice;
use portunus_env::{Decision, Env};
use serde::{Deserialize, Serialize};

pub trait Policy<O>: Send + Sync {
    fn act(&self, decision: &Decision<O>) -> Choice;
}

/// May cache work between calls (e.g. reuse a search tree), but caches
/// must be pure optimizations: the result is what an empty cache would
/// produce.
pub trait SearchPolicy<E: Env> {
    fn search(&mut self, env: &E, decision: &Decision<E::Obs>) -> SearchResult;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub choice: Choice,
    /// Visit share per action-space index; the policy training target.
    pub visits: Vec<f32>,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MctsConfig {
    pub simulations: u32,
    pub c_puct: f32,
    pub dirichlet_alpha: f32,
    pub dirichlet_frac: f32,
    pub temperature: f32,
    /// Stop expanding this far past the root.
    pub horizon: SimDuration,
}

/// Names an observation encoding and action space together. A model only
/// makes sense on the schema it was trained under, and samples from
/// different schemas must never be mixed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaId(pub String);

/// A trained network: features in, action priors and a value out.
pub trait Model: Send + Sync {
    fn version(&self) -> ModelVersion;
    fn schema(&self) -> &SchemaId;
    fn input_width(&self) -> usize;
    fn output_width(&self) -> usize;
    fn evaluate(&self, batch: &[&[f32]]) -> Vec<Evaluation>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub priors: Vec<f32>,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ModelVersion {
    pub name: String,
    pub step: u64,
}

/// Builds the policy for each seat. Policies are stateless, so one build
/// can serve any number of parallel rollouts.
pub trait PolicyFactory<O>: Send + Sync {
    fn name(&self) -> &str;
    fn build(&self, seat: Seat, model: Option<Arc<dyn Model>>) -> Arc<dyn Policy<O>>;
}
