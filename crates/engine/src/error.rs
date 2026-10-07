use portunus_core::{ActorId, AuraId, EnemyKey, PetId, Seat, SimTime, SpellId};
use portunus_gamedata::GameBuild;
use thiserror::Error;

use crate::mask::Readiness;

#[derive(Debug, Clone, Error)]
pub enum EngineError {
    #[error("invalid setup: {0:?}")]
    Setup(Vec<SetupIssue>),
    #[error("no decision is pending")]
    NotAwaiting,
    #[error("seat {0:?} is not the pending decision")]
    WrongSeat(Seat),
    #[error("illegal choice for seat {seat:?}: {reason:?}")]
    Illegal { seat: Seat, reason: IllegalChoice },
    /// The seat waited twice on the same trigger with nothing in between.
    #[error("livelock at seat {0:?}")]
    Livelock(Seat),
    /// The run needs a feature this kernel doesn't implement yet.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Mechanics kept reacting to their own reactions without settling.
    #[error("event cascade did not settle at {0:?}")]
    Runaway(SimTime),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupIssue {
    NoSeats,
    TooManySeats(usize),
    BuildMismatch { game: GameBuild, enemies: GameBuild },
    UnknownSpell(SpellId),
    UnknownAura(AuraId),
    UnknownPet(PetId),
    UnknownEnemy(EnemyKey),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IllegalChoice {
    NotAnAbility(SpellId),
    NotReady {
        ability: SpellId,
        readiness: Readiness,
    },
    /// The target selector matched nothing.
    NoValidTarget,
    NotATarget(ActorId),
    HostileBeforePull(SpellId),
    NothingToStop,
    NoChannelTick,
    NotCancelable(AuraId),
    /// The target is beyond the spell's range.
    OutOfRange(ActorId),
    /// Dead, out of combat, or being moved by force.
    CantMove,
    /// The goal is already met or names nothing reachable.
    NoMoveGoal,
    /// Already moving toward this goal.
    AlreadyMoving,
    /// No voluntary move to stop.
    NotMoving,
}
