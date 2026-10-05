//! Enemy definitions: what each enemy type is and does.
//!
//! An enemy's behavior is a list of rules. Each rule has a trigger, an
//! optional repeat interval, and an action. Rule clocks are relative to the
//! enemy's own engagement, not to the pull. Routes reference enemies only by
//! [`EnemyKey`]; nothing about behavior is authored per route.

use portunus_core::{AuraId, Dist, EnemyKey, EventName, SimDuration, Trigger};
use serde::{Deserialize, Serialize};

use crate::stats::SchoolMask;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyDef {
    pub key: EnemyKey,
    pub name: String,
    pub kind: EnemyKind,
    pub health: Dist<f64>,
    /// Enemy-forces contribution.
    pub forces: u32,
    pub defense: EnemyDefense,
    #[serde(default)]
    pub initial_phase: Option<PhaseName>,
    #[serde(default)]
    pub rules: Vec<EnemyRule>,
}

/// What reduces incoming player damage and crits.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnemyDefense {
    /// Levels above the player (bosses are typically higher than trash).
    pub level_offset: i8,
    pub armor: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyKind {
    Trash,
    Boss,
    /// Only ever appears via `SpawnAdds`.
    Add,
}

/// A named boss phase.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PhaseName(pub String);

/// Index into [`EnemyDef::rules`]; the engine's handle for a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RuleIndex(pub u16);

/// What an enemy rule's trigger may refer to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemySubject {
    /// This enemy.
    Itself,
    /// Everything engaged in the same combat.
    Combat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyRule {
    /// Stable name that plan anchors and triggers refer to.
    pub name: EventName,
    /// Only active while the enemy is in this phase.
    #[serde(default)]
    pub phase: Option<PhaseName>,
    pub when: Trigger<EnemySubject>,
    /// Re-fire on this interval after the first firing, while active.
    #[serde(default)]
    pub repeat: Option<Dist<SimDuration>>,
    pub action: EnemyAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyAction {
    Damage {
        amount: Dist<f64>,
        school: SchoolMask,
        target: EnemyTarget,
    },
    /// A cast bar; the payload lands at the end unless interrupted.
    Cast {
        time: SimDuration,
        interruptible: bool,
        then: Box<EnemyAction>,
    },
    /// Players in `target` must move: non-instant casts stop and only
    /// spells castable while moving are usable for the duration.
    ForceMovement {
        duration: Dist<SimDuration>,
        target: EnemyTarget,
    },
    /// Apply an aura from [`crate::GameData::auras`] to this enemy, e.g. an
    /// immunity shield or an enrage.
    SelfAura(AuraId),
    SpawnAdds {
        adds: Vec<(EnemyKey, u32)>,
    },
    EnterPhase(PhaseName),
    Sequence(Vec<EnemyAction>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyTarget {
    Tank,
    RandomPlayer,
    RandomNonTank,
    /// This many distinct random players.
    RandomPlayers(u8),
    AllPlayers,
}
