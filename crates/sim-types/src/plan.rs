//! The Plan Artifact (minimal shape).
//!
//! Immutable data mediating between planner and policies (invariant 10).
//! Cooldown assignments in a run file compile into [`PlannedWindow`]s here:
//! they are *windows the policy reads*, never forced casts — the assigned
//! player may be mid-cast, moving, or dead, and the policy owns execution.

use serde::{Deserialize, Serialize};

use crate::time::SimTime;

/// A full plan for one run: per-pull commitments plus chance constraints.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub pulls: Vec<PullPlan>,
    pub constraints: PlanConstraints,
}

impl Plan {
    /// Plan entry for a pull, by its stable name.
    #[must_use]
    pub fn pull(&self, name: &str) -> Option<&PullPlan> {
        self.pulls.iter().find(|p| p.pull == name)
    }
}

/// Commitments for one pull.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullPlan {
    /// Pull name (matches `ResolvedCombat::name`).
    pub pull: String,
    pub lust: bool,
    /// Planned cooldown windows for this pull.
    pub windows: Vec<PlannedWindow>,
}

/// One planned cooldown window: player X should use ability Y around anchor Z.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedWindow {
    /// Roster index of the assigned player.
    pub player: u8,
    /// Ability name in that player's kit (resolved to a slot by the policy
    /// layer against its codex).
    pub ability: String,
    pub anchor: WindowAnchor,
}

/// What a planned window is anchored to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WindowAnchor {
    /// The pull's combat start.
    PullStart,
    /// A fixed offset from combat start.
    Offset(SimTime),
    /// A named enemy script occurrence, e.g. barkskin on `overseer#1/slam@2`.
    ScriptEvent { spawn: String, script: String, occurrence: u32 },
}

/// Chance constraints the plan must respect; reported alongside scores,
/// never silently folded into them (invariant 9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanConstraints {
    /// Maximum acceptable per-pull death probability.
    pub death_prob_max: f64,
}

impl Default for PlanConstraints {
    fn default() -> Self {
        Self { death_prob_max: 0.05 }
    }
}
