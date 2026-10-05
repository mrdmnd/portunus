//! Getting data in.
//!
//! Sources (client data dumps, the SimulationCraft spell dump, hand-written
//! files) produce [`GameData`] and [`EnemyData`]; combat logs calibrate
//! enemy timings and damage. Messy outside formats stop here and never leak
//! into the simulator.
//!
//! [`RonFile`] reads hand-written tables; [`check_game_data`] and
//! [`check_enemy_data`] catch dangling references in whatever was loaded.

mod check;
mod ron_file;

pub use check::{check_enemy_data, check_game_data, DataIssue, Owner};
pub use ron_file::{read_ron, RonFile};

use portunus_core::{EnemyKey, EventName, SimTime};
use portunus_gamedata::{EnemyData, GameBuild, GameData};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("io: {0}")]
    Io(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("build mismatch: expected {expected:?}, found {found:?}")]
    BuildMismatch {
        expected: GameBuild,
        found: GameBuild,
    },
}

pub trait GameDataSource {
    fn load(&self) -> Result<GameData, IngestError>;
}

pub trait EnemyDataSource {
    fn load(&self) -> Result<EnemyData, IngestError>;
}

/// One enemy ability seen in a log, relative to that enemy's engagement.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedEvent {
    pub enemy: EnemyKey,
    pub event: EventName,
    pub at: SimTime,
    pub damage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservedCombat {
    pub build: GameBuild,
    pub events: Vec<ObservedEvent>,
}

pub trait LogSource {
    fn combats(&self) -> Result<Vec<ObservedCombat>, IngestError>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationReport {
    /// Per enemy rule: how far the data's timing was from the logs.
    pub timing_error_ms: Vec<(EnemyKey, EventName, f64)>,
}

pub trait Calibrator {
    fn calibrate(&self, data: &mut EnemyData, logs: &[ObservedCombat]) -> CalibrationReport;
}
