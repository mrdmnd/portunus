//! Spells: the things players press.

use portunus_core::{SimDuration, SpellId};
use serde::{Deserialize, Serialize};

use crate::effect::Effect;
use crate::stats::{Cost, SchoolMask};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpellDef {
    pub id: SpellId,
    pub name: String,
    pub school: SchoolMask,
    pub cast: CastKind,
    pub gcd: Option<GcdDef>,
    pub cooldown: Option<CooldownDef>,
    pub costs: Vec<Cost>,
    pub targeting: Targeting,
    /// Projectile flight time: effects resolve on impact, and the spell is
    /// "in flight" until then.
    pub travel: Option<SimDuration>,
    /// Usable while forced to move (instants and a few exceptions).
    pub castable_while_moving: bool,
    /// Usable while another cast is in progress (off-GCD, off-cast).
    pub usable_while_casting: bool,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastKind {
    Instant,
    Cast {
        time: SimDuration,
        hasted: bool,
    },
    /// Effects run on each tick rather than at completion.
    Channel {
        duration: SimDuration,
        ticks: u8,
        hasted: bool,
    },
    /// Charged, then released at a chosen stage. The spell's own effects run
    /// on release, followed by the reached stage's.
    Empower {
        /// Charge time to reach each stage, cumulative.
        stages: Vec<SimDuration>,
        hasted: bool,
        /// How long the final stage can be held before it releases itself.
        hold: SimDuration,
        /// Index `n - 1` holds stage `n`'s extra effects.
        stage_effects: Vec<Vec<Effect>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcdDef {
    pub base: SimDuration,
    pub hasted: bool,
    pub floor: SimDuration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CooldownDef {
    pub duration: SimDuration,
    pub charges: u8,
    pub hasted: bool,
    /// Spells sharing a category share one cooldown.
    pub category: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Targeting {
    None,
    SelfOnly,
    Enemy,
    Ally,
}
