//! The composition root.
//!
//! A run file ([`RunConfig`]) names the game and enemy data, a scenario, an
//! optional plan, and per seat a loadout, a policy (a function in
//! [`scripts`], by name), and a latency model. [`Bundle::load`] validates
//! the game, enemies, scenario, kits, loadouts, and policy names. Plans are
//! currently deserialized but not validated. The bundle then builds the
//! pieces the other crates only define:
//!
//! - [`Source`], the [`portunus_env::EpisodeSource`];
//! - [`SimEnv`], the [`portunus_env::Env`] over the kernel and
//!   [`portunus_mechanics::PartyMechanics`], observed by [`ScriptObserver`]
//!   and scored by [`PartyMeter`] and [`NegElapsed`];
//! - [`SimArm`], the [`portunus_eval::Arm`] that plays whole episodes.
//!
//! The `portunus-run` binary runs a file's seeds through
//! [`portunus_eval::LocalRunner`] and prints the [`report::render`] report.
//! Its `compare` mode plays two policies for one seat on the same seeds
//! ([`compare()`]) and prints [`report::render_comparison`].

mod arm;
mod bundle;
mod compare;
mod env;
mod observe;
mod opt;
mod parity;
pub mod report;
pub mod scripts;
mod train;

pub use arm::{breakdown, AbilityRow, Breakdown, SimArm};
pub use bundle::{Bundle, RunConfig, RunError, SeatConfig};
pub use compare::{compare, CompareError, CompareSpec, Comparison, Verdict};
pub use env::{NegElapsed, PartyMeter, SimEnv, Source};
pub use observe::{
    AllyObs, AuraObs, BattleResObs, ChannelObs, CooldownObs, DemandObs, EmpowerObs, GroundObs,
    MovementObs, PetObs, ScriptObserver, SeatObs, SeatObsFeaturizer, TargetObs, FOREVER,
};
pub use opt::{optimize_stats, BundleStatObjective, OptimizeReport, StatPoint, StatSearchSpace};
pub use parity::{
    check_simc_parity, default_simc_profile, AbilityParityRow, SimcParityReport,
    DEFAULT_PARITY_TOLERANCE,
};
pub use train::{train_rl, IterationSummary, TrainReport, TrainSpec};
