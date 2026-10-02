//! The plan: group-level commitments made before the run.
//!
//! A decision variable, not part of the world: it can be searched over like a
//! loadout. Windows are *advice the policy reads* through its observation,
//! never forced casts; the assigned player may be mid-cast, moving, or dead,
//! and the policy owns execution. Bloodlust is just a window on the lust
//! spell.

use std::collections::BTreeMap;

use portunus_core::{EventName, PullName, Seat, SimDuration, SpawnLabel, SpellId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub pulls: BTreeMap<PullName, PullPlan>,
    pub constraints: PlanConstraints,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PullPlan {
    pub windows: Vec<PlannedWindow>,
}

/// Seat X should use spell Y around anchor Z.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedWindow {
    pub seat: Seat,
    pub spell: SpellId,
    pub anchor: Anchor,
    /// How early or late still counts as on plan.
    pub tolerance: SimDuration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    PullStart,
    Offset(SimDuration),
    SpawnEngaged(SpawnLabel),
    /// The `nth` firing (1-based) of a spawn's named rule, e.g. defensive on
    /// the boss's second slam.
    EnemyEvent {
        spawn: SpawnLabel,
        event: EventName,
        nth: u32,
    },
}

/// Limits reported alongside scores, never folded into them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanConstraints {
    /// Maximum acceptable chance of any death in a pull.
    pub max_death_prob_per_pull: Option<f64>,
}
