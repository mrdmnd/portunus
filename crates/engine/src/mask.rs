//! Legality, as the engine sees it.

use portunus_core::{ActorId, AuraId, SimDuration};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    Now,
    /// Becomes legal after this long if nothing changes (cooldown, GCD,
    /// resource regen).
    In(SimDuration),
    /// Not reachable without some other event (missing proc, no target).
    Blocked,
}

/// Per-slot readiness for one seat; index is the ability slot. Waiting is
/// always legal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionMask {
    pub slots: Vec<Readiness>,
    /// A cast or channel is in progress and may be stopped.
    pub can_stop: bool,
    /// A channel is in progress and has a tick left to wait for.
    pub can_wait_tick: bool,
    /// Live enemies `SetTarget` may pick.
    pub targets: Vec<ActorId>,
    /// Auras the seat holds and may cancel.
    pub cancel: Vec<AuraId>,
}
