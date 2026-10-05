//! Hand-written data files in RON.

use std::path::{Path, PathBuf};

use portunus_gamedata::{EnemyData, GameData};
use serde::de::DeserializeOwned;

use crate::{EnemyDataSource, GameDataSource, IngestError};

/// One RON file holding a whole table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RonFile {
    pub path: PathBuf,
}

impl RonFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl GameDataSource for RonFile {
    fn load(&self) -> Result<GameData, IngestError> {
        read_ron(&self.path)
    }
}

impl EnemyDataSource for RonFile {
    fn load(&self) -> Result<EnemyData, IngestError> {
        read_ron(&self.path)
    }
}

/// Any RON-encoded value, e.g. a scenario or a loadout.
pub fn read_ron<T: DeserializeOwned>(path: &Path) -> Result<T, IngestError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| IngestError::Io(format!("{}: {e}", path.display())))?;
    ron::from_str(&text).map_err(|e| IngestError::Parse(format!("{}: {e}", path.display())))
}
