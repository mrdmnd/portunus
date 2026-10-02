//! Decision points.

use portunus_core::{AbilitySlot, ActorId, AuraId, Seat, SimTime};
use serde::{Deserialize, Serialize};

use crate::outcome::Outcome;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Step {
    Decide(DecisionRequest),
    Done(Outcome),
}

/// One seat's turn within a decision batch. Every request in a batch shares
/// the same `now` and observes the same state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRequest {
    pub seat: Seat,
    pub reason: WakeReason,
    /// The seat asked to be woken for this (a wait it named, its own cast or
    /// GCD ending) rather than being interrupted by something new.
    pub anticipated: bool,
    /// When the waking event happened. Earlier than `now` by the seat's
    /// latency draw.
    pub event_at: SimTime,
    pub now: SimTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeReason {
    /// The pre-pull window before a combat opened (anticipated).
    PrePull,
    CombatStart,
    CastEnd,
    /// The seat stopped its own cast and is free again.
    CastStopped,
    /// The seat took a free action (`SetTarget`, `CancelAura`) and is asked
    /// again at the same timestamp.
    FreeAction,
    /// A tick of the seat's channel resolved (`Wait::ChannelTick` or
    /// `CastOpts::tick_wakes`).
    ChannelTick,
    GcdEnd,
    /// The seat just started a GCD-triggering cast and an off-GCD ability
    /// is ready right now.
    OffGcdReady,
    CooldownReady(AbilitySlot),
    AuraGained(AuraId),
    AuraLost(AuraId),
    WaitElapsed,
    ConditionMet,
    EnemyEngaged(ActorId),
    EnemyDied(ActorId),
    EnemyCastStart(ActorId),
    MovementStart,
    MovementEnd,
    /// Requested by mechanics (e.g. a proc that changes priorities).
    Mechanics,
}
