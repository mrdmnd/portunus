//! Classes: what one brings to its group, whichever spec plays it.

use portunus_core::AuraId;
use serde::{Deserialize, Serialize};

/// Raid buffs and debuffs a class provides. A run applies those of every
/// class among its seats, each once however many seats share the class.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClassDef {
    /// Matches [`crate::spec::SpecDef::class`].
    pub name: String,
    /// On every seat for the whole run (Arcane Intellect).
    #[serde(default)]
    pub group_auras: Vec<AuraId>,
    /// On every enemy once it engages (Mystic Touch).
    #[serde(default)]
    pub enemy_auras: Vec<AuraId>,
    /// On every living seat as each pull starts (Bloodlust).
    #[serde(default)]
    pub on_pull: Vec<PullAura>,
}

/// An aura granted to the group at the start of a pull.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PullAura {
    pub aura: AuraId,
    /// Applied alongside `aura`, which a seat already holding it doesn't
    /// get (Sated after Bloodlust).
    #[serde(default)]
    pub lockout: Option<AuraId>,
}
