//! The composition root.
//!
//! A run file ([`RunConfig`]) names the game and enemy data, a scenario, an
//! optional plan, and per seat a loadout, a policy (a function in
//! [`scripts`], by name), and a latency model. [`Bundle::load`] reads and checks all of it; the bundle
//! then builds the pieces the other crates only define:
//!
//! - [`Source`], the [`portunus_env::EpisodeSource`];
//! - [`SimEnv`], the [`portunus_env::Env`] over the kernel and
//!   [`portunus_mechanics::PartyMechanics`], observed by [`ScriptObserver`]
//!   and scored by [`PartyMeter`] and [`NegElapsed`];
//! - [`SimArm`], the [`portunus_eval::Arm`] that plays whole episodes.
//!
//! The `portunus-run` binary runs a file's seeds through
//! [`portunus_eval::LocalRunner`] and prints the [`report::render`] report.

mod arm;
mod bundle;
mod env;
mod observe;
pub mod report;
pub mod scripts;

pub use arm::{breakdown, AbilityRow, Breakdown, SimArm};
pub use bundle::{Bundle, RunConfig, RunError, SeatConfig};
pub use env::{NegElapsed, PartyMeter, SimEnv, Source};
pub use observe::{
    AuraObs, ChannelObs, CooldownObs, DemandObs, EmpowerObs, MovementObs, PetObs, ScriptObserver,
    SeatObs, TargetObs, FOREVER,
};
