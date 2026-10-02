//! The decision-making boundary between the simulator and anything that
//! plays it.
//!
//! An [`Env`] wraps an engine with three choices made here, not in the
//! engine or the policy: what each seat may see ([`observe`]), how choices
//! are indexed for learning ([`action_space`]), and what counts as progress
//! ([`reward`]). Swapping any of them is an experiment; the engine
//! underneath stays fixed.
//!
//! The reward is shared by the whole party: the objective is the group's
//! total time, not any one seat's damage.
//!
//! Observations are a pure function of the current state plus static
//! context; nothing here remembers. Under [`InfoSet::Realistic`] that makes
//! decisions only approximately Markov (some truth is hidden), and we
//! accept that rather than adding memory.

pub mod action_space;
pub mod episode;
pub mod observe;
pub mod reward;

use portunus_core::{Seat, Seed, SimDuration};
use portunus_engine::{ActionMask, Choice, EngineError, Outcome, WakeReason};
use thiserror::Error;

pub use action_space::ActionSpace;
pub use episode::{Episode, EpisodeSource};
pub use observe::{InfoSet, ObsContext, Observer};
pub use reward::{Progress, ProgressMeter, Reward};

#[derive(Debug, Clone)]
pub struct Decision<O> {
    pub seat: Seat,
    pub reason: WakeReason,
    pub obs: O,
    pub legal: ActionMask,
    pub key: DecisionKey,
}

/// Addresses a stochastic policy's random draws (`Purpose::Policy`), so
/// randomness is a function of the decision rather than of hidden policy
/// state. `index` counts this seat's decisions in the episode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecisionKey {
    pub seed: Seed,
    pub seat: Seat,
    pub index: u64,
}

#[derive(Debug, Clone)]
pub enum Turn<O> {
    Decide(Decision<O>),
    Done(Outcome),
}

#[derive(Debug, Clone)]
pub struct Transition<O> {
    /// Reward accrued between this choice and the next decision.
    pub reward: f64,
    pub elapsed: SimDuration,
    pub next: Turn<O>,
}

#[derive(Debug, Error)]
pub enum EnvError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error("env has not been reset")]
    NotStarted,
    #[error("episode already finished")]
    Finished,
}

/// `Clone` is the fork: search policies copy the env and play ahead.
pub trait Env: Clone {
    type Obs: Clone;
    fn reset(&mut self, seed: Seed) -> Result<Turn<Self::Obs>, EnvError>;
    fn step(&mut self, choice: Choice) -> Result<Transition<Self::Obs>, EnvError>;
}
