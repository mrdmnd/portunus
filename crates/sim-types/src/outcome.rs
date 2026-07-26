//! Rollout outcomes (SCAFFOLD.md §2). All scoring over these lives in
//! `sim-eval` (invariant 9); this is just the record.

use serde::{Deserialize, Serialize};

use crate::ids::ActorId;
use crate::time::SimTime;

/// Result of one rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// Time the last enemy died; `None` = wipe or timeout.
    pub kill_time: Option<SimTime>,
    /// Player deaths during the rollout.
    pub deaths: u32,
    /// Per-player detail rows.
    pub per_actor: Vec<ActorOutcome>,
}

/// Per-player rollout detail.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ActorOutcome {
    pub actor: ActorId,
    pub damage_done: f64,
    pub casts: u32,
    pub died: bool,
}
