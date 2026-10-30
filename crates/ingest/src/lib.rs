//! Getting data in.
//!
//! Sources produce [`GameData`] and [`EnemyData`] without leaking their
//! outside formats into the simulator. [`RonFile`] is the checked-in source;
//! [`GameDataSource`], [`EnemyDataSource`], [`LogSource`], and [`Calibrator`]
//! are extension points for generated tables and combat-log calibration.
//!
//! [`RonFile`] reads hand-written tables; [`check_game_data`] and
//! [`check_enemy_data`] catch dangling references in whatever was loaded.

mod check;
pub mod dump;
pub mod gear;
pub mod items;
pub mod modifiers;
mod ron_file;
pub mod talent_string;

pub use check::{check_enemy_data, check_game_data, DataIssue, Owner};
pub use dump::{
    audit_game_data, audit_talents, sync_game_data_with_dump, DumpDiff, DumpEffect, DumpSpell,
    DumpSyncedSource, SimcEquippedSlot, SimcPopulatedLoadout, SimcProfile, SpellDump,
    TalentAuditFinding, TalentFindingKind,
};
pub use gear::{EquippedGear, GearLibrary, UnmodelledEffect};
pub use items::{GearEffect, GearSource, ItemTables, ItemTrigger};
pub use ron_file::{merge_derived, read_ron, RonFile};
pub use talent_string::{DecodedTalents, TalentPick, TraitTable, TraitTree};

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
