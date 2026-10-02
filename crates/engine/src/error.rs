use portunus_core::Seat;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("setup: {0}")]
    Setup(String),
    #[error("no decision is pending")]
    NotAwaiting,
    #[error("seat {0:?} is not the pending decision")]
    WrongSeat(Seat),
    #[error("illegal choice: {0}")]
    Illegal(String),
    /// The seat waited for nothing in particular twice on the same trigger.
    #[error("livelock at seat {0:?}")]
    Livelock(Seat),
}
