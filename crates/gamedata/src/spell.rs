//! Spells: the things players press.

use portunus_core::{AuraId, SimDuration, SpellId};
use serde::{Deserialize, Serialize};

use crate::effect::Effect;
use crate::item::WeaponHand;
use crate::stats::{Cost, SchoolMask};

/// `gcd` (`None` is off the GCD) and `hostile` must always be written out;
/// fields with defaults may be omitted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpellDef {
    pub id: SpellId,
    pub name: String,
    pub school: SchoolMask,
    pub cast: CastKind,
    pub gcd: Option<GcdDef>,
    #[serde(default)]
    pub cooldown: Option<CooldownDef>,
    #[serde(default)]
    pub costs: Vec<Cost>,
    pub targeting: Targeting,
    /// Harms enemies or starts combat. Only non-hostile spells are usable
    /// before a pull begins.
    pub hostile: bool,
    /// Farthest an enemy target may be, in yards (`None`: no limit).
    #[serde(default)]
    pub range: Option<f64>,
    /// Missile speed in yards per second. A spell travels if it has a speed
    /// or a `min_travel`: effects resolve on impact, and it is "in flight"
    /// until then. Direct damage is rolled at launch, as SimC snapshots at
    /// execute, unless `rolls_on_impact`.
    #[serde(default)]
    pub speed: Option<f64>,
    /// The shortest flight, however close the target. Alone, a fixed delay
    /// (spell data's "missile speed is delay", e.g. Meteor).
    #[serde(default)]
    pub min_travel: SimDuration,
    /// Roll direct damage as the projectile lands instead (Lava Burst,
    /// whose Flame Shock crit is checked on impact).
    #[serde(default)]
    pub rolls_on_impact: bool,
    /// Usable while forced to move (instants and a few exceptions).
    #[serde(default)]
    pub castable_while_moving: bool,
    /// Usable while another cast is in progress (off-GCD, off-cast).
    #[serde(default)]
    pub usable_while_casting: bool,
    /// A strike with this weapon: its hits are melee hits, which
    /// `WeaponHit` listeners (poisons) react to.
    #[serde(default)]
    pub weapon: Option<WeaponHand>,
    /// All must hold for the spell to be usable.
    #[serde(default)]
    pub requires: Vec<Requirement>,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// A condition on the caster, or its target, for a spell to be usable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    /// The caster holds one of these: a form (Shred in Cat Form), stealth
    /// or what stands in for it (Ambush in Stealth, Vanish, or Shadow
    /// Dance). A form listed here keeps the spell castable in it.
    AnyAura(Vec<AuraId>),
    /// The caster holds none of these.
    NoAura(Vec<AuraId>),
    /// Only between pulls (Stealth, Prowl).
    OutOfCombat,
    /// The target has at most this fraction of its max health (`0.2` is
    /// 20%): Execute, Kill Shot. Inclusive, as SimC's `target_ready`
    /// refuses only above it (`health_percentage() > execute_pct`). Checked
    /// against the primary target for readiness, and against the chosen
    /// target when cast; a seat is woken when its primary target crosses
    /// it.
    TargetHpAtMost(f64),
    /// The target has at least this fraction of its max health (Kill
    /// Shot's upper window with some talents).
    TargetHpAtLeast(f64),
    /// The caster holds at least this many stacks of the aura.
    CasterStacksAtLeast { aura: AuraId, stacks: u8 },
}

impl Requirement {
    /// Whether it's about the target rather than the caster.
    pub fn on_target(&self) -> bool {
        matches!(self, Self::TargetHpAtMost(_) | Self::TargetHpAtLeast(_))
    }
}

impl SpellDef {
    pub fn travels(&self) -> bool {
        self.speed.is_some() || self.min_travel > SimDuration::ZERO
    }

    /// Flight time to a target `distance` yards away; `None` if it doesn't
    /// travel.
    pub fn flight_time(&self, distance: f64) -> Option<SimDuration> {
        if !self.travels() {
            return None;
        }
        let flying = self.speed.filter(|s| *s > 0.0).map_or(0.0, |s| {
            (distance.max(0.0) / s * 1000.0)
                .round()
                .min(f64::from(u32::MAX / 2))
        });
        Some(SimDuration(flying as u32).max(self.min_travel))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastKind {
    Instant,
    Cast {
        time: SimDuration,
        hasted: bool,
    },
    /// Succeeds as it starts (costs, cooldown, cast listeners); its effects
    /// run on each tick, the last at the end of `duration`. Haste is
    /// fixed at the start (SimC drops `STATE_HASTE` from channels' update
    /// flags).
    Channel {
        duration: SimDuration,
        ticks: u8,
        hasted: bool,
        /// Auto-attacks keep landing through the channel. Otherwise, as
        /// in SimC (`interrupt_auto_attack`, on by default), swings that
        /// come due during it keep their rhythm but do nothing.
        #[serde(default)]
        swings: bool,
    },
    /// Charged, then released at a chosen stage. Costs and cooldown are paid
    /// as it starts (SimC runs the charge as a channel); on release the GCD
    /// starts again and the spell's own effects run, followed by the
    /// reached stage's. Released before the first stage, it fizzles
    /// (SimC's `last_tick` skips the release at `EMPOWER_NONE`). Haste
    /// scales the stage times and the hold alike, fixed at the start.
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
    /// Spells sharing a category share one cooldown: its charges, its
    /// timer, and cooldown adjustments to any of them. Every spell in a
    /// category must agree on `duration`, `charges`, and `hasted`. Like
    /// SimC's shared "potion" cooldown, a category's cooldown starts when
    /// the spell is used; nothing waits for combat to end.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn missile(speed: Option<f64>, min_travel: u32) -> SpellDef {
        SpellDef {
            id: SpellId(1),
            name: "missile".into(),
            school: SchoolMask(1),
            cast: CastKind::Instant,
            gcd: None,
            cooldown: None,
            costs: Vec::new(),
            targeting: Targeting::Enemy,
            hostile: true,
            range: None,
            speed,
            min_travel: SimDuration(min_travel),
            rolls_on_impact: false,
            castable_while_moving: false,
            usable_while_casting: false,
            weapon: None,
            requires: Vec::new(),
            effects: Vec::new(),
        }
    }

    #[test]
    fn flight_time_is_distance_over_speed_with_a_floor() {
        assert_eq!(missile(None, 0).flight_time(20.0), None);
        assert_eq!(
            missile(Some(60.0), 0).flight_time(20.0),
            Some(SimDuration(333))
        );
        assert_eq!(
            missile(Some(60.0), 500).flight_time(20.0),
            Some(SimDuration(500))
        );
        assert_eq!(
            missile(Some(60.0), 500).flight_time(60.0),
            Some(SimDuration(1000))
        );
        assert_eq!(
            missile(None, 1000).flight_time(40.0),
            Some(SimDuration(1000))
        );
        assert_eq!(
            missile(Some(50.0), 0).flight_time(-5.0),
            Some(SimDuration::ZERO)
        );
    }
}
