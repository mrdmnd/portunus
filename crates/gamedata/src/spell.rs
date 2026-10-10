//! Spells: the things players press.

use portunus_core::{SimDuration, SpellId};
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
    #[serde(default)]
    pub effects: Vec<Effect>,
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
