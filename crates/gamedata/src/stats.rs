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
    /// `(threshold, fraction kept above it)`, ascending. Thresholds are in
    /// percent before diminishing returns; each band keeps its own fraction.
    #[serde(default)]
    pub diminishing: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatCurves {
    pub ratings: BTreeMap<Stat, RatingCurve>,
    /// Item stat budget by item level.
    pub item_budget: BTreeMap<u16, f64>,
    /// Crit before any rating, in percent.
    pub base_crit_pct: f64,
    pub spell_power_per_intellect: f64,
    /// Per point of agility or strength, whichever is the spec's primary.
    pub attack_power_per_primary: f64,
    pub health_per_stamina: f64,
    /// Physical damage taken is reduced by `armor / (armor + armor_constant)`.
    pub armor_constant: f64,
    /// Auto-attacks of an actor wielding two weapons miss this often, in
    /// percent. Nothing else misses, glances, or is dodged or parried:
    /// players hit enemies from behind.
    #[serde(default = "dual_wield_miss_pct")]
    pub dual_wield_miss_pct: f64,
}

fn dual_wield_miss_pct() -> f64 {
    19.0
}

/// Bitmask over magic schools; multi-school spells set several bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchoolMask(pub u8);

impl SchoolMask {
    pub const PHYSICAL: Self = Self(1);
    pub const HOLY: Self = Self(2);
    pub const FIRE: Self = Self(4);
    pub const NATURE: Self = Self(8);
    pub const FROST: Self = Self(16);
    pub const SHADOW: Self = Self(32);
    pub const ARCANE: Self = Self(64);
}

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
/// rate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceDef {
    pub kind: ResourceKind,
    pub max: f64,
    pub initial: f64,
    pub regen_per_sec: f64,
    pub regen_hasted: bool,
}
