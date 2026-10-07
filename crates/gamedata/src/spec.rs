//! Specializations.

use portunus_core::{AuraId, SpecId, SpellId};
use serde::{Deserialize, Serialize};

use crate::stats::{ResourceDef, Stat, StatBlock};
use crate::talent::TalentTree;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Tank,
    Healer,
    Damage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpecDef {
    pub id: SpecId,
    pub class: String,
    pub name: String,
    pub role: Role,
    pub primary_stat: Stat,
    pub base_stats: StatBlock,
    /// Mastery before any rating, in percent.
    pub base_mastery_pct: f64,
    /// Mastery percent per percent of converted mastery rating; specs'
    /// masteries scale differently.
    pub mastery_coef: f64,
    pub resources: Vec<ResourceDef>,
    pub baseline_spells: Vec<SpellId>,
    /// Always-on spec passives, including mastery.
    pub baseline_auras: Vec<AuraId>,
    /// Class tree and spec tree. Hero trees are shared between specs and
    /// live in `GameData::hero_trees`.
    #[serde(default)]
    pub trees: Vec<TalentTree>,
}
