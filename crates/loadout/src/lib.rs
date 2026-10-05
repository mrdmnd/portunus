//! Pre-combat configuration: one of the three things being optimized.
//!
//! A [`Loadout`] is what a player chooses before the key starts. Compiling it
//! against [`GameData`] yields an [`ActorTemplate`]: final stats, the
//! abilities the seat can press, and the permanent auras that carry every
//! passive effect. Compilation is pure and cheap, because configuration
//! search calls it constantly. [`Compiler`] is the implementation.

mod compile;

use std::collections::{BTreeMap, BTreeSet};

use portunus_core::{AuraId, ItemId, PetId, SpecId, SpellId, TalentId};
use portunus_gamedata::item::{GearSlot, WeaponDef};
use portunus_gamedata::spec::Role;
use portunus_gamedata::stats::{DerivedStats, ResourceDef, StatBlock};
use portunus_gamedata::GameData;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use compile::Compiler;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    pub spec: SpecId,
    #[serde(default)]
    pub gear: BTreeMap<GearSlot, EquippedItem>,
    #[serde(default)]
    pub talents: TalentSelection,
    #[serde(default)]
    pub consumables: Vec<ItemId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquippedItem {
    pub item: ItemId,
    pub item_level: u16,
    #[serde(default)]
    pub enchant: Option<AuraId>,
    #[serde(default)]
    pub gems: Vec<ItemId>,
}

/// Talent id to chosen rank.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TalentSelection(pub BTreeMap<TalentId, u8>);

/// Everything the engine needs to instantiate one player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorTemplate {
    pub spec: SpecId,
    pub role: Role,
    pub stats: StatBlock,
    pub derived: DerivedStats,
    /// Every spell the seat can press, by the id it was granted under:
    /// baseline and talent spells, equipped on-use items, and consumables.
    pub abilities: BTreeSet<SpellId>,
    /// Applied at run start and never expire: spec passives, talents,
    /// set bonuses, item effects, enchants, consumables.
    pub passive_auras: Vec<AuraId>,
    pub resources: Vec<ResourceDef>,
    pub main_hand: Option<WeaponDef>,
    pub off_hand: Option<WeaponDef>,
    /// Summoned at run start (a hunter's or warlock's chosen pet).
    pub permanent_pet: Option<PetId>,
    /// The configuration this was compiled from: static, inspectable facts
    /// (talents, set pieces, trinkets) that policies may condition on.
    pub source: Loadout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadoutIssue {
    UnknownSpec(SpecId),
    UnknownItem(ItemId),
    UnknownAura(AuraId),
    /// The stat curves have no budget for this item level.
    UnknownItemLevel(u16),
    WrongSlot {
        slot: GearSlot,
        item: ItemId,
    },
    NotConsumable(ItemId),
    NotAGem(ItemId),
    UniqueConflict {
        a: ItemId,
        b: ItemId,
    },
    /// Two equipped items or consumables grant the same on-use spell, so
    /// one ability would fire both.
    DuplicateOnUse(SpellId),
    UnknownTalent(TalentId),
    TalentRankTooHigh {
        talent: TalentId,
        rank: u8,
    },
    TalentUnreachable(TalentId),
    /// Two talents from the same choice node.
    ChoiceConflict {
        a: TalentId,
        b: TalentId,
    },
    TooManyTalentPoints {
        tree: String,
        spent: u8,
        allowed: u8,
    },
}

#[derive(Debug, Error)]
pub enum LoadoutError {
    #[error("invalid loadout: {0:?}")]
    Invalid(Vec<LoadoutIssue>),
}

pub trait LoadoutCompiler {
    /// Every problem, not just the first.
    fn validate(&self, data: &GameData, loadout: &Loadout) -> Vec<LoadoutIssue>;
    fn compile(&self, data: &GameData, loadout: &Loadout) -> Result<ActorTemplate, LoadoutError>;
}

/// Simple loadouts for party members whose roles aren't being optimized,
/// compiled like any other.
pub trait StandIns {
    fn loadout(&self, data: &GameData, role: Role) -> Loadout;
}
