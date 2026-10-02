//! Stats, schools, resources, and rating curves.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stat {
    Strength,
    Agility,
    Intellect,
    Stamina,
    CritRating,
    HasteRating,
    MasteryRating,
    VersatilityRating,
    Armor,
}

/// The four rated secondaries, as percentages after conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RatedStat {
    Crit,
    Haste,
    Mastery,
    Versatility,
}

/// A bag of stat values (ratings, not percentages).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StatBlock(pub BTreeMap<Stat, f64>);

/// Percentages and derived values after rating conversion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct DerivedStats {
    pub crit_pct: f64,
    pub haste_pct: f64,
    pub mastery_pct: f64,
    pub versatility_pct: f64,
    pub attack_power: f64,
    pub spell_power: f64,
    pub max_health: f64,
}

/// Rating-to-percent conversion with diminishing returns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingCurve {
    pub rating_per_pct: f64,
    /// `(pct threshold, fraction kept above it)`, ascending.
    pub diminishing: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatCurves {
    pub ratings: BTreeMap<Stat, RatingCurve>,
    /// Item stat budget by item level.
    pub item_budget: BTreeMap<u16, f64>,
}

/// Bitmask over magic schools; multi-school spells set several bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchoolMask(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Health,
    Mana,
    Rage,
    Energy,
    Focus,
    RunicPower,
    Runes,
    ComboPoints,
    HolyPower,
    SoulShards,
    AstralPower,
    Maelstrom,
    Insanity,
    Chi,
    Fury,
    ArcaneCharges,
    Essence,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceAmount {
    pub kind: ResourceKind,
    pub amount: f64,
}

/// A spell's resource cost. `amount` gates the cast; `extra` is consumed on
/// top when available (finishers spending every combo point, Ferocious Bite's
/// extra energy, Execute's extra rage).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub kind: ResourceKind,
    pub amount: f64,
    pub extra: f64,
    pub scaling: SpendScaling,
}

/// How the amount actually spent scales the spell's effects.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpendScaling {
    None,
    /// Multiply by the total spent (per combo point, per holy power).
    PerUnit,
    /// Multiply by `1 + coef * extra_spent / extra`.
    Extra(f64),
}

/// A spec's resource pool. Between events it evolves linearly at its regen
/// rate, so wake-up times are solved exactly instead of polled.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceDef {
    pub kind: ResourceKind,
    pub max: f64,
    pub initial: f64,
    pub regen_per_sec: f64,
    pub regen_hasted: bool,
}
