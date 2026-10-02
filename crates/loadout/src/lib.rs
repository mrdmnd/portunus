//! Pre-combat configuration: one of the three things being optimized.
//!
//! A [`Loadout`] is what a player chooses before the key starts. Compiling it
//! against [`GameData`] yields an [`ActorTemplate`]: final stats, the ability
//! list (whose order defines the policy's action slots), and the permanent
//! auras that carry every passive effect. Compilation is pure and cheap,
//! because configuration search calls it constantly.

use std::collections::BTreeMap;

use portunus_core::{AbilitySlot, AuraId, ItemId, PetId, SpecId, SpellId, TalentId};
use portunus_gamedata::item::{GearSlot, WeaponDef};
use portunus_gamedata::spec::Role;
use portunus_gamedata::stats::{DerivedStats, ResourceDef, StatBlock};
use portunus_gamedata::GameData;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Loadout {
    pub spec: SpecId,
    pub gear: BTreeMap<GearSlot, EquippedItem>,
    pub talents: TalentSelection,
    pub consumables: Vec<ItemId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquippedItem {
    pub item: ItemId,
    pub item_level: u16,
    pub enchant: Option<AuraId>,
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
    /// Index in this list is the [`portunus_core::AbilitySlot`].
    pub abilities: Vec<SpellId>,
    /// Applied at combat start and never expire: spec passives, talents,
    /// set bonuses, item effects, enchants, consumables.
    pub passive_auras: Vec<AuraId>,
    pub resources: Vec<ResourceDef>,
    pub main_hand: Option<WeaponDef>,
    pub off_hand: Option<WeaponDef>,
    /// Summoned at run start (a hunter's or warlock's chosen pet).
    pub permanent_pet: Option<PetId>,
    /// Which ability slot fires each equipped on-use item.
    pub item_slots: BTreeMap<GearSlot, AbilitySlot>,
    /// The configuration this was compiled from: static, inspectable facts
    /// (talents, set pieces, trinkets) that policies may condition on.
    pub source: Loadout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadoutIssue {
    UnknownSpec(SpecId),
    UnknownItem(ItemId),
    WrongSlot {
        slot: GearSlot,
        item: ItemId,
    },
    UniqueConflict {
        a: ItemId,
        b: ItemId,
    },
    UnknownTalent(TalentId),
    TalentRankTooHigh {
        talent: TalentId,
        rank: u8,
    },
    TalentUnreachable(TalentId),
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
