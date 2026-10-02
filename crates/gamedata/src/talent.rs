//! Talent trees and what talents grant.

use portunus_core::{AuraId, SpellId, TalentId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TalentTree {
    pub name: String,
    pub points: u8,
    pub nodes: Vec<TalentNode>,
    /// `(row, points spent above it required)`.
    pub gates: Vec<(u8, u8)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TalentNode {
    pub row: u8,
    /// More than one entry makes this a choice node.
    pub choices: Vec<TalentId>,
    pub max_rank: u8,
    /// At least one of these must be taken first.
    pub requires_any: Vec<TalentId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TalentDef {
    pub id: TalentId,
    pub name: String,
    /// What each rank grants, cumulatively.
    pub ranks: Vec<Vec<Grant>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grant {
    Spell(SpellId),
    PassiveAura(AuraId),
    ReplaceSpell { from: SpellId, to: SpellId },
}
