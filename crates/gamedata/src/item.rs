//! Items, weapons, and item sets.

use portunus_core::{AuraId, ItemId, ItemSetId, SimDuration, SpellId};
use serde::{Deserialize, Serialize};

use crate::stats::Stat;

/// Where an item can go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipKind {
    Head,
    Neck,
    Shoulder,
    Back,
    Chest,
    Wrist,
    Hands,
    Waist,
    Legs,
    Feet,
    Finger,
    Trinket,
    OneHand,
    TwoHand,
    MainHandOnly,
    OffHand,
    /// Not equipped: food, flasks, potions, runes.
    Consumable,
    Gem,
}

/// A concrete equipment slot on a character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GearSlot {
    Head,
    Neck,
    Shoulder,
    Back,
    Chest,
    Wrist,
    Hands,
    Waist,
    Legs,
    Feet,
    Finger1,
    Finger2,
    Trinket1,
    Trinket2,
    MainHand,
    OffHand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: ItemId,
    pub name: String,
    pub equip: EquipKind,
    /// Share of the item-level stat budget per stat.
    pub stat_allocation: Vec<(Stat, f64)>,
    pub weapon: Option<WeaponDef>,
    /// Always-on effects while equipped (procs live on these auras).
    pub passive_auras: Vec<AuraId>,
    /// A player-triggered effect; the policy decides when.
    pub on_use: Option<SpellId>,
    pub set: Option<ItemSetId>,
    /// Only one equipped item may share this group (e.g. unique-equipped).
    pub unique_group: Option<u32>,
}

/// Auto-attacks: each equipped weapon swings on its own timer at the
/// holder's target while in combat. Hasted by attack speed; hard casts and
/// channels pause melee swings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WeaponDef {
    pub speed: SimDuration,
    pub min_damage: f64,
    pub max_damage: f64,
    /// Bows, guns, and wands (auto shot) rather than melee swings.
    pub ranged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponHand {
    MainHand,
    OffHand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemSetDef {
    pub id: ItemSetId,
    pub name: String,
    /// `(pieces required, aura granted)`.
    pub bonuses: Vec<(u8, AuraId)>,
}
