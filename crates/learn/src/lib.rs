//! Self-play and training.
//!
//! The simulator side collects [`Trajectory`]s; a [`Trainer`] (most likely
//! Python, behind bindings or a batch file format) turns them into new model
//! versions held by a [`ModelStore`]. Samples are plain numbers so the
//! boundary stays language-neutral.

use std::sync::Arc;

use portunus_core::{Seat, Seed};
use portunus_engine::Outcome;
use portunus_policy::{Model, ModelVersion};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub seat: Seat,
    pub features: Vec<f32>,
    pub legal: Vec<bool>,
    /// Search visit shares.
    pub policy_target: Vec<f32>,
    /// Discounted return from this decision.
    pub value_target: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trajectory {
    pub seed: Seed,
    pub model: ModelVersion,
    pub samples: Vec<Sample>,
    pub outcome: Outcome,
}

/// Plays episodes with a search policy and records what it did.
pub trait Collector {
    fn collect(&mut self, model: Arc<dyn Model>, seeds: &[Seed]) -> Vec<Trajectory>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TrainStats {
    pub policy_loss: f32,
    pub value_loss: f32,
    pub samples_seen: u64,
}

pub trait Trainer {
    fn train(&mut self, batch: &[Sample]) -> TrainStats;
    fn checkpoint(&mut self) -> ModelVersion;
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("unknown model version {0:?}")]
    Unknown(ModelVersion),
    #[error("io: {0}")]
    Io(String),
}

pub trait ModelStore {
    fn latest(&self, name: &str) -> Option<ModelVersion>;
    fn load(&self, version: &ModelVersion) -> Result<Arc<dyn Model>, StoreError>;
}
