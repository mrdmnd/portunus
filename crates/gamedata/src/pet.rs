//! Pets and guardians: allied actors owned by a seat.
//!
//! They are actors, not seats. They never get decision points: each one runs
//! its [`PetDef::autocast`] priority on its owner's target, auto-attacks with
//! its own swing timer, and casts on command (`Effect::CommandPet`). Their
//! stats follow the owner's, so owner buffs reach them through scaling
//! rather than duplicated auras.

use portunus_core::{AuraId, PetId, SpellId};
use serde::{Deserialize, Serialize};

use crate::item::WeaponDef;
use crate::stats::ResourceDef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PetDef {
    pub id: PetId,
    pub name: String,
    pub kind: PetKind,
    pub scaling: PetScaling,
    pub melee: Option<WeaponDef>,
    /// Cast in this order whenever ready, on the owner's target.
    pub autocast: Vec<SpellId>,
    pub passive_auras: Vec<AuraId>,
    pub resources: Vec<ResourceDef>,
    /// Summoning past this many active copies replaces the oldest.
    pub max_active: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PetKind {
    /// Stays until dismissed or killed (a hunter's pet, a felguard).
    Permanent,
    /// Timed, from a summoning effect (imps, Xuen, a ghoul army).
    Guardian,
}

/// Shares of the owner's stats. Pets also inherit the owner's crit, haste,
/// mastery, and versatility percentages.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PetScaling {
    pub attack_power: f64,
    pub spell_power: f64,
    pub health: f64,
}
