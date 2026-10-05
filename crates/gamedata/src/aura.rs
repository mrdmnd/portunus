//! Auras: buffs, debuffs, DoTs, and every passive effect.
//!
//! Ground effects (Consecration, Death and Decay, Efflorescence) are auras
//! on the caster whose periodic effects hit `AllEnemies` or the `Party`;
//! anything that cares whether you "stand in it" checks the caster aura.

use portunus_core::{AuraId, SimDuration, SpellId};
use serde::{Deserialize, Serialize};

use crate::effect::{Coefficient, Effect, Listener, Modifier};
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
    pub cap: Option<Coefficient>,
    /// When the value reaches this, `on_threshold` runs and the value drops
    /// by the threshold (e.g. a "deal X damage to proc" counter).
    pub threshold: Option<Coefficient>,
    pub on_threshold: Vec<Effect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuraValueKind {
    /// Absorbs incoming damage of these schools; removed at zero.
    Absorb { school: SchoolMask },
    /// Each periodic tick draws from the value; the drawn amount is the
    /// tick's `Coefficient::EventAmount` (Ignite, Stagger, Deep Wounds
    /// style banks).
    Bank(BankDraw),
    /// Just a number for effects and conditions to read.
    Counter,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BankDraw {
    /// Value divided by the ticks remaining.
    SpreadOverRemaining,
    /// This fraction of the current value.
    Fraction(f64),
}
