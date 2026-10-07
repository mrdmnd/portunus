//! Pets, guardians, and totems: allied actors owned by a seat.
//!
//! Following SimC's `pet_t`, all three are actors of their own whose stats
//! come from the owner's through [`PetScaling`], so owner buffs reach them
//! without duplicated auras. They differ in lifetime and in who drives them
//! (see [`PetKind`]).

use portunus_core::{AuraId, PetId, SimDuration, SpellId};
use serde::{Deserialize, Serialize};

use crate::item::WeaponDef;
use crate::stats::ResourceDef;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PetDef {
    pub id: PetId,
    pub name: String,
    pub kind: PetKind,
    pub scaling: PetScaling,
    #[serde(default)]
    pub melee: Option<WeaponDef>,
    /// Cast in this order whenever ready, at the owner's target. This is
    /// the whole pet bar: abilities a player triggers by hand are the
    /// owner's spells that use `Effect::CommandPet` (Kill Command, Command
    /// Demon).
    #[serde(default)]
    pub autocast: Vec<SpellId>,
    /// Applied by the pet to itself on arrival. A totem's pulse is a
    /// periodic passive aura.
    #[serde(default)]
    pub passive_auras: Vec<AuraId>,
    #[serde(default)]
    pub resources: Vec<ResourceDef>,
    /// Lifetime when the summoning effect names none; ignored for
    /// [`PetKind::Pet`].
    #[serde(default)]
    pub duration: Option<SimDuration>,
    /// Summoning past this many active copies replaces the oldest.
    #[serde(default)]
    pub max_active: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PetKind {
    /// Controllable and persistent: stays until dismissed or killed, one at
    /// a time per owner (summoning another dismisses it), and answers the
    /// owner's command spells as well as autocasting (a hunter's pet, a
    /// felguard, a Primal Elemental).
    Pet,
    /// Temporary and uncontrolled: lives for its duration and acts only on
    /// its autocast and on effects that drive it (imps, Xuen, Fire
    /// Elemental, Farseer's Ancestors).
    Guardian,
    /// A guardian that stays where it was dropped and works by pulsing
    /// through its passive auras instead of casting (Liquid Magma Totem,
    /// Healing Stream Totem).
    Totem,
}

impl PetKind {
    /// Whether this kind lives for a duration rather than until dismissed.
    pub fn is_temporary(self) -> bool {
        !matches!(self, PetKind::Pet)
    }
}

/// Shares of the owner's power and health, as SimC's `owner_coeff`.
///
/// Pets and guardians also inherit the owner's crit, haste, and
/// versatility, including the owner's unscoped crit and haste modifiers,
/// but not mastery or the owner's damage modifiers: owner auras reach them
/// only through `ModKind::PetDamagePct` and `GuardianDamagePct`. Totems
/// ignore this and act with the owner's own stats and modifiers, since
/// their spells count as the owner's.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct PetScaling {
    #[serde(default)]
    pub sp_from_sp: f64,
    #[serde(default)]
    pub sp_from_ap: f64,
    #[serde(default)]
    pub ap_from_ap: f64,
    #[serde(default)]
    pub ap_from_sp: f64,
    /// Of the owner's maximum health.
    #[serde(default)]
    pub health: f64,
}
