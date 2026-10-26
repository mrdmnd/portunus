//! Hand-written data files in RON.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use portunus_core::{AuraId, SpellId};
use portunus_gamedata::aura::AuraDef;
use portunus_gamedata::race::{Race, RaceDef};
use portunus_gamedata::spell::SpellDef;
use portunus_gamedata::{EnemyData, GameData};
use serde::de::DeserializeOwned;
use serde::Deserialize;

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
        let mut data = read_ron(&self.path)?;
        if self.path.file_name().is_some_and(|name| name == "game.ron") {
            if let Some(parent) = self.path.parent() {
                let racials = parent.join("racials.ron");
                if racials.is_file() {
                    merge_racials(&mut data, &racials)?;
                }
            }
        }
        Ok(data)
    }
}

/// Spells, auras, and the race table checked in beside `game.ron`.
#[derive(Debug, Deserialize)]
struct RacialSlice {
    races: BTreeMap<Race, RaceDef>,
    spells: BTreeMap<SpellId, SpellDef>,
    auras: BTreeMap<AuraId, AuraDef>,
}

fn merge_racials(data: &mut GameData, path: &Path) -> Result<(), IngestError> {
    let slice: RacialSlice = read_ron(path)?;
    for (id, spell) in slice.spells {
        if data.spells.contains_key(&id) {
            return Err(IngestError::Parse(format!(
                "{}: spell {} is already in game data",
                path.display(),
                id.0
            )));
        }
        data.spells.insert(id, spell);
    }
    for (id, aura) in slice.auras {
        if data.auras.contains_key(&id) {
            return Err(IngestError::Parse(format!(
                "{}: aura {} is already in game data",
                path.display(),
                id.0
            )));
        }
        data.auras.insert(id, aura);
    }
    data.races = slice.races;
    Ok(())
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
