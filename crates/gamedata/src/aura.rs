//! Auras: buffs, debuffs, DoTs, and every passive effect.
//!
//! Ground effects (Consecration, Death and Decay, Efflorescence) are auras
//! on the caster whose periodic effects hit `AllEnemies` or the `Party`;
//! anything that cares whether you "stand in it" checks the caster aura.

use portunus_core::{AuraId, SimDuration, SpellId};
use serde::{Deserialize, Serialize};

use crate::effect::{Coefficient, Effect, Listener, Modifier};
use crate::item::WeaponDef;
use crate::stats::SchoolMask;

/// Fields with defaults may be omitted from authored files; `duration` may
/// not, so a forgotten duration can't silently make an aura permanent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuraDef {
    pub id: AuraId,
    pub name: String,
    /// `None` is permanent (talents, tier bonuses, mastery).
    pub duration: Option<SimDuration>,
    #[serde(default = "one")]
    pub max_stacks: u8,
    #[serde(default)]
    pub refresh: RefreshRule,
    #[serde(default)]
    pub periodic: Option<Periodic>,
    #[serde(default)]
    pub value: Option<AuraValue>,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
    #[serde(default)]
    pub listeners: Vec<Listener>,
    /// While active, pressing `from` casts `to` (e.g. Ascendance turning
    /// Chain Lightning into Lava Beam). The ability is still `from`.
    #[serde(default)]
    pub overrides: Vec<(SpellId, SpellId)>,
    #[serde(default)]
    pub on_expire: Vec<Effect>,
    /// The holder may remove it (`Choice::CancelAura`).
    #[serde(default)]
    pub cancelable: bool,
    /// Can't be applied while the target holds this aura (Bloodlust and
    /// Exhaustion).
    #[serde(default)]
    pub blocked_by: Option<AuraId>,
    /// A shapeshift or stance.
    #[serde(default)]
    pub form: Option<FormDef>,
    /// Stealth, or something that behaves like it (Vanish, Prowl).
    #[serde(default)]
    pub stealth: Option<StealthDef>,
    /// Removed when the holder loses this aura (Prowl with Cat Form).
    #[serde(default)]
    pub ends_with: Option<AuraId>,
    /// Kept when the holder dies, and through its recovery between pulls
    /// (Sated, Exhaustion, Temporal Displacement, Fatigued: dying doesn't
    /// let the group lust again early).
    #[serde(default)]
    pub persists_through_death: bool,
    /// A caster keeps it on one holder at a time: applying it anew takes
    /// the caster's instance off any other (Hunter's Mark).
    #[serde(default)]
    pub unique_per_source: bool,
    /// A hit that would kill the holder, a seat, leaves it alive instead
    /// (Cauterize, Cheat Death, Ardent Defender).
    #[serde(default)]
    pub prevents_death: Option<PreventDeath>,
    /// Placed on the ground (Death and Decay, Consecration, Rain of Fire):
    /// its ticks and expiry effects reach only enemies inside the area,
    /// and `Predicate::InOwnGround` asks whether its caster still stands in
    /// it.
    #[serde(default)]
    pub ground: Option<GroundDef>,
}

/// Where a ground aura reaches. It is centred where its spell's target
/// stood when cast, or else on the enemy its holder was targeting: seats
/// have no place of their own, so a self-cast area is taken to be at the
/// enemy the caster is fighting.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GroundDef {
    /// Yards from the centre. As in SimC's ground effects, an enemy's own
    /// combat reach counts on top.
    pub radius: f64,
}

/// What happens instead of the holder's death. As SimC's Ardent Defender
/// (`sc_paladin_protection.cpp`, the only death prevention SimC models):
/// the lethal hit deals nothing past setting health *to* `heal_to_pct` of
/// maximum, which can raise it. The aura stays unless `on_prevent` removes
/// it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreventDeath {
    /// In `(0, 1]`.
    pub heal_to_pct: f64,
    /// Applied to the holder by itself as it saves it; while the holder
    /// has it (from anyone) the aura saves nothing.
    #[serde(default)]
    pub lockout: Option<AuraId>,
    /// Run as the aura's effects: its source casts them at the holder.
    #[serde(default)]
    pub on_prevent: Vec<Effect>,
}

/// A shapeshift or stance. Gaining a form removes any other of its group
/// from the holder. Casting a spell the form doesn't allow leaves it first,
/// as a druid shifts out to cast Wrath.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormDef {
    /// Forms of a group exclude each other (a druid's shapeshifts, a
    /// warrior's stances).
    pub group: u8,
    /// Every spell is castable in it (Moonkin Form, stances); otherwise
    /// only `allows` and spells whose `requires` name this form are.
    #[serde(default)]
    pub allows_all: bool,
    #[serde(default)]
    pub allows: Vec<SpellId>,
    /// Replaces the holder's weapons while held: one main hand and no off
    /// hand (Cat Form's 1.0 s paws).
    #[serde(default)]
    pub weapon: Option<WeaponDef>,
}

/// While held, the holder doesn't auto-attack. It breaks, running
/// `on_break`, on a hostile cast (an instant once its effects resolve, so
/// openers keep their stealth bonus; a cast or channel as it starts) and,
/// if `breaks_on_damage`, when the holder takes damage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StealthDef {
    /// Hostile spells that don't break it (Sap).
    #[serde(default)]
    pub keeps: Vec<SpellId>,
    #[serde(default = "yes")]
    pub breaks_on_damage: bool,
    /// Not run when it expires or is cancelled (Subterfuge's lingering
    /// window).
    #[serde(default)]
    pub on_break: Vec<Effect>,
}

fn yes() -> bool {
    true
}

pub(crate) fn one() -> u8 {
    1
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshRule {
    #[default]
    Replace,
    /// Carry over remaining time up to this fraction of the base duration.
    Pandemic(f64),
    /// Add the full duration to what remains.
    Extend,
    /// Each stack expires on its own timer, a full duration after it was
    /// applied. The aura's expiry is its newest stack's.
    Ironfur,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Periodic {
    pub period: SimDuration,
    pub hasted: bool,
    /// Deal a partial final tick when the duration isn't a whole number of
    /// periods.
    pub partial_final_tick: bool,
    pub effects: Vec<Effect>,
}

/// A number carried by an aura instance: absorb remaining, a damage bank, or
/// a counter. Changed by `Effect::AddAuraValue` and `ConsumeAuraValue`;
/// read via `Coefficient::AuraValue`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuraValue {
    pub kind: AuraValueKind,
    /// The value it starts with when first applied, from its source (and
    /// for `PctMaxHealth`, its holder): a shield's size, e.g. an enemy's
    /// `SelfAura` absorb of a share of its own health.
    #[serde(default)]
    pub initial: Option<Coefficient>,
    /// The value never exceeds this (e.g. Ignite-style bank caps).
    pub cap: Option<Coefficient>,
    /// Each time the value reaches this (after the cap), it drops by it
    /// and `on_threshold` runs, cast by the aura's source at its holder
    /// (Seed of Corruption, "deal X damage to proc" counters).
    pub threshold: Option<Coefficient>,
    pub on_threshold: Vec<Effect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuraValueKind {
    /// Absorbs incoming damage of these schools before health, smallest
    /// shield first, and is removed when used up. Damage it soaks doesn't
    /// count as damage done (as in SimC), but still procs as a hit.
    Absorb { school: SchoolMask },
    /// Each periodic tick draws from the value before its effects run; the
    /// drawn amount is the tick's `Coefficient::EventAmount`, and isn't
    /// scaled again for a partial final tick (Ignite, Stagger, Deep Wounds
    /// style banks). Needs `periodic`. Filling it doesn't refresh the
    /// duration; pair `AddAuraValue` with `ApplyAura` for that.
    Bank(BankDraw),
    /// Just a number for effects and conditions to read.
    Counter,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BankDraw {
    /// The value over the ticks remaining, this one included and a partial
    /// final tick counted by its fraction (SimC's `residual_action`), so
    /// the last tick empties it. A permanent aura draws it all.
    SpreadOverRemaining,
    /// This fraction of the current value, in `(0, 1]`; scaled down on a
    /// partial final tick.
    Fraction(f64),
}
