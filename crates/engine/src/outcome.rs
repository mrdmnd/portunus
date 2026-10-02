//! What a finished rollout reports.

use portunus_core::{PullName, Seat, SimTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    /// Every pull cleared before its timeout with someone alive.
    pub completed: bool,
    pub end_time: SimTime,
    pub pulls: Vec<PullOutcome>,
    pub seats: Vec<SeatOutcome>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullOutcome {
    pub pull: PullName,
    pub started: SimTime,
    /// `None` if the run ended during this pull.
    pub cleared: Option<SimTime>,
    pub deaths: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatOutcome {
    pub seat: Seat,
    pub damage_done: f64,
    pub casts: u32,
    pub deaths: u32,
}
