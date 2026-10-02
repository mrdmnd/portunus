//! Versioned game data. Pure data, no behavior.
//!
//! Two tables, both pinned to a [`GameBuild`]:
//!
//! - [`GameData`]: everything player-side — specs, spells, auras, items,
//!   talents, and the stat curves that turn ratings into percentages;
//! - [`EnemyData`]: every enemy type and its behavior rules.
//!
//! Abstraction rule: anything passive (a talent, a tier bonus, a trinket's
//! proc, mastery) is a permanent [`aura::AuraDef`] carrying modifiers and
//! listeners. Behavior that the data vocabulary can't express is a named
//! [`portunus_core::HookKey`], implemented in code by a spec kit.

pub mod aura;
pub mod effect;
pub mod enemy;
pub mod item;
pub mod pet;
pub mod spec;
pub mod spell;
pub mod stats;
pub mod talent;

use std::collections::BTreeMap;

use portunus_core::{AuraId, EnemyKey, ItemId, ItemSetId, PetId, SpecId, SpellId, TalentId};
use serde::{Deserialize, Serialize};

pub use aura::AuraDef;
pub use enemy::EnemyDef;
pub use item::{ItemDef, ItemSetDef};
pub use pet::PetDef;
pub use spec::SpecDef;
pub use spell::SpellDef;
pub use stats::StatCurves;
pub use talent::TalentDef;

/// The game version a table was extracted from and calibrated against.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GameBuild {
    /// e.g. `11.2.5`.
    pub version: String,
    pub build: u32,
}

/// Player-side game data for one build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameData {
    pub build: GameBuild,
    pub specs: BTreeMap<SpecId, SpecDef>,
    pub spells: BTreeMap<SpellId, SpellDef>,
    pub auras: BTreeMap<AuraId, AuraDef>,
    pub items: BTreeMap<ItemId, ItemDef>,
    pub item_sets: BTreeMap<ItemSetId, ItemSetDef>,
    pub talents: BTreeMap<TalentId, TalentDef>,
    pub pets: BTreeMap<PetId, PetDef>,
    pub curves: StatCurves,
}

/// Enemy-side game data for one build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyData {
    pub build: GameBuild,
    pub enemies: BTreeMap<EnemyKey, EnemyDef>,
}
