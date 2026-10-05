//! Self-play and training.
//!
//! The simulator side collects [`Trajectory`]s; a [`Trainer`] turns them
//! into new model versions held by a [`ModelStore`]. Samples are plain
//! numbers so the trainer can live in another language.

use std::sync::Arc;

use portunus_core::{Seat, Seed};
use portunus_engine::Outcome;
use portunus_policy::{Model, ModelVersion, SchemaId};
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
    /// The encoding every sample in this trajectory uses.
    pub schema: SchemaId,
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

/// A trained model, serialized by the trainer in its own format.
#[derive(Debug, Clone, PartialEq)]
pub struct Checkpoint {
    pub version: ModelVersion,
    pub schema: SchemaId,
    pub weights: Vec<u8>,
}

pub trait Trainer {
    fn schema(&self) -> &SchemaId;
    /// Every sample must come from a trajectory with this trainer's schema.
    fn train(&mut self, batch: &[Sample]) -> TrainStats;
    fn checkpoint(&mut self) -> Checkpoint;
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("unknown model version {0:?}")]
    Unknown(ModelVersion),
    #[error("version {0:?} already stored")]
    Exists(ModelVersion),
    #[error("io: {0}")]
    Io(String),
}

pub trait ModelStore {
    fn latest(&self, name: &str) -> Option<ModelVersion>;
    fn load(&self, version: &ModelVersion) -> Result<Arc<dyn Model>, StoreError>;
    /// Versions are immutable once saved.
    fn save(&mut self, checkpoint: &Checkpoint) -> Result<(), StoreError>;
}
