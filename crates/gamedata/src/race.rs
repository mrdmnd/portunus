//! Playable races and the racial spells and auras they grant.
//!
//! Spell ids match SimulationCraft's `racial_spells.inc` for build
//! 12.1.5.70575. A racial whose combat effect the vocabulary can say is a
//! spell or an aura in `data/racials.ron`. Profession, travel, and cosmetic
//! racials are still granted, with no combat modifier.

use portunus_core::{AuraId, SpellId};
use serde::{Deserialize, Serialize};

/// A playable race. Faction variants that share a racial kit (Pandaren,
/// Dracthyr, Earthen) are one variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Race {
    Human,
    Orc,
    Dwarf,
    NightElf,
    Undead,
    Tauren,
    Gnome,
    Troll,
    Goblin,
    BloodElf,
    Draenei,
    Worgen,
    Pandaren,
    Nightborne,
    HighmountainTauren,
    MagharOrc,
    ZandalariTroll,
    KulTiran,
    DarkIronDwarf,
    LightforgedDraenei,
    VoidElf,
    Mechagnome,
    Vulpera,
    Dracthyr,
    Earthen,
    Haranir,
}

impl Race {
    /// SimulationCraft's `race=` token, including faction aliases that share
    /// a kit.
    pub fn from_simc(name: &str) -> Option<Self> {
        Some(match name {
            "human" => Self::Human,
            "orc" => Self::Orc,
            "dwarf" => Self::Dwarf,
            "night_elf" => Self::NightElf,
            "undead" | "scourge" => Self::Undead,
            "tauren" => Self::Tauren,
            "gnome" => Self::Gnome,
            "troll" => Self::Troll,
            "goblin" => Self::Goblin,
            "blood_elf" => Self::BloodElf,
            "draenei" => Self::Draenei,
            "worgen" => Self::Worgen,
            "pandaren" | "pandaren_alliance" | "pandaren_horde" => Self::Pandaren,
            "nightborne" => Self::Nightborne,
            "highmountain_tauren" => Self::HighmountainTauren,
            "maghar_orc" => Self::MagharOrc,
            "zandalari_troll" => Self::ZandalariTroll,
            "kul_tiran" => Self::KulTiran,
            "dark_iron_dwarf" => Self::DarkIronDwarf,
            "lightforged_draenei" => Self::LightforgedDraenei,
            "void_elf" => Self::VoidElf,
            "mechagnome" => Self::Mechagnome,
            "vulpera" => Self::Vulpera,
            "dracthyr" | "dracthyr_alliance" | "dracthyr_horde" => Self::Dracthyr,
            "earthen" | "earthen_dwarf" | "earthen_alliance" | "earthen_horde" => Self::Earthen,
            "haranir" => Self::Haranir,
            _ => return None,
        })
    }
}

/// Zandalari Embrace of the Loa. Pa'ku, Gonk, Krag'wa, Bwonsamdi, and
/// Kimbul are the loa the 12.1.5 racial spells still publish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Loa {
    Paku,
    Gonk,
    Kragwa,
    Bwonsamdi,
    Kimbul,
}

impl Loa {
    pub fn from_simc(name: &str) -> Option<Self> {
        Some(match name {
            "paku" | "pa_ku" | "embrace_of_paku" => Self::Paku,
            "gonk" | "embrace_of_gonk" => Self::Gonk,
            "kragwa" | "embrace_of_kragwa" => Self::Kragwa,
            "bwonsamdi" | "embrace_of_bwonsamdi" => Self::Bwonsamdi,
            "kimbul" | "embrace_of_kimbul" => Self::Kimbul,
            _ => return None,
        })
    }
}

/// Night elf Touch of Elune follows the time of day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Day,
    Night,
}

impl TimeOfDay {
    pub fn from_simc(name: &str) -> Option<Self> {
        Some(match name {
            "day" => Self::Day,
            "night" => Self::Night,
            _ => return None,
        })
    }
}

/// The Khaz Algar gem an Earthen has ingested. It selects which Well Fed
/// stat Ingest Minerals would grant. The size of that meal is not a single
/// published number, so the choice is recorded without a stat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EarthenGem {
    Amber,
    Emerald,
    Onyx,
    Ruby,
    Sapphire,
}

impl EarthenGem {
    pub fn from_simc(name: &str) -> Option<Self> {
        Some(match name {
            "amber" => Self::Amber,
            "emerald" => Self::Emerald,
            "onyx" => Self::Onyx,
            "ruby" => Self::Ruby,
            "sapphire" => Self::Sapphire,
            _ => return None,
        })
    }
}

/// When a racial aura stands in for the one listed in [`RaceDef::auras`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuraAltWhen {
    /// The spec's primary stat is intellect (Touch of the Grave's spell
    /// power coefficient).
    Intellect,
    /// Night elf Touch of Elune at night.
    Night,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuraAlt {
    pub when: AuraAltWhen,
    pub instead_of: AuraId,
    pub aura: AuraId,
}

/// One race's kit. Class-specific rows are Blood Fury, Arcane Torrent,
/// Gift of the Naaru, and Expansive Mind, matched on [`crate::spec::SpecDef::class`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RaceDef {
    pub id: Race,
    pub name: String,
    /// Actives every class of this race can press.
    #[serde(default)]
    pub spells: Vec<SpellId>,
    /// Permanent auras every class of this race holds.
    #[serde(default)]
    pub auras: Vec<AuraId>,
    /// `(class name, spell)`.
    #[serde(default)]
    pub class_spells: Vec<(String, SpellId)>,
    /// `(class name, aura)`.
    #[serde(default)]
    pub class_auras: Vec<(String, AuraId)>,
    #[serde(default)]
    pub aura_alts: Vec<AuraAlt>,
    /// Zandalari loa, granted when the loadout picks one.
    #[serde(default)]
    pub loas: Vec<(Loa, AuraId)>,
    /// Earthen ingested gem, granted when the loadout picks one.
    #[serde(default)]
    pub gems: Vec<(EarthenGem, AuraId)>,
}
