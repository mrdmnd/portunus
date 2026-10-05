use portunus_core::{ActorId, AuraId, EnemyKey, PetId, Seat, SpellId};
use portunus_gamedata::GameBuild;
use thiserror::Error;

use crate::mask::Readiness;

#[derive(Debug, Error)]
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
}
