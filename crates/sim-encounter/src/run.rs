//! The run-file ("pull sheet") schema, parsed from TOML.
//!
//! Deliberately contains no mob behavior — enemies are referenced by codex
//! name. What it does contain: pull order, engagement waves, travel time
//! between pulls, and static cooldown assignments.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sim_types::JitteredMs;
use thiserror::Error;

/// Run-file load errors (validation happens in [`crate::RunSampler::new`]).
#[derive(Debug, Error)]
pub enum RunSpecError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] toml::de::Error),
}

/// A whole run definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunSpec {
    pub name: String,
    /// Informational codex reference (build pinning lands with `sim-codex`).
    #[serde(default)]
    pub codex: Option<String>,
    /// Enemy-forces requirement; validated against the summed count of
    /// everything pulled, if set.
    #[serde(default)]
    pub count_requirement: Option<u32>,
    pub pulls: Vec<PullSpec>,
}

impl RunSpec {
    pub fn from_toml_str(src: &str) -> Result<Self, RunSpecError> {
        Ok(toml::from_str(src)?)
    }

    pub fn from_path(path: &std::path::Path) -> Result<Self, RunSpecError> {
        Self::from_toml_str(&std::fs::read_to_string(path)?)
    }
}

/// One pull: one combat, possibly with several engagement waves (a chained
/// pull, or trash pulled onto a boss, is just multiple waves here).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullSpec {
    /// Stable name: plans, traces, and diffs address the pull by this.
    pub name: String,
    /// Travel/downtime before this pull starts.
    #[serde(default)]
    pub travel_in: Option<JitteredMs>,
    /// Wipe deadline for this combat.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub lust: bool,
    pub engage: Vec<EngageWave>,
    /// Static cooldown assignments, compiled into Plan windows.
    #[serde(default)]
    pub cooldowns: Vec<CooldownAssignment>,
}

fn default_timeout_ms() -> u64 {
    360_000
}

/// One engagement wave: which mobs, and when their clocks start.
/// Exactly one of `after_ms` / `when` may be set; neither means the wave is
/// part of the opening gather.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngageWave {
    /// Enemy codex name -> how many. BTreeMap keeps instance labels
    /// (`grunt#1`, `grunt#2`, ...) deterministic.
    pub mobs: BTreeMap<String, u32>,
    /// Timer relative to combat start (planned body-pull, patrol arrival).
    #[serde(default)]
    pub after_ms: Option<u64>,
    /// Chain condition over combat state.
    #[serde(default)]
    pub when: Option<WhenSpec>,
}

/// A chain condition. Exactly one field must be set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WhenSpec {
    /// Remaining HP fraction of everything engaged so far drops below this.
    #[serde(default)]
    pub engaged_hp_frac_below: Option<f64>,
    /// At most this many engaged mobs remain alive.
    #[serde(default)]
    pub engaged_alive_at_most: Option<u32>,
    /// Combat-relative time reaches this many milliseconds.
    #[serde(default)]
    pub time_at_least_ms: Option<u64>,
    /// A specific spawn (instance label within this pull) has died.
    #[serde(default)]
    pub spawn_dead: Option<String>,
}

impl WhenSpec {
    /// How many condition fields are set (must be exactly 1).
    #[must_use]
    pub fn arity(&self) -> usize {
        usize::from(self.engaged_hp_frac_below.is_some())
            + usize::from(self.engaged_alive_at_most.is_some())
            + usize::from(self.time_at_least_ms.is_some())
            + usize::from(self.spawn_dead.is_some())
    }
}

/// A static cooldown assignment: player X plans ability Y around anchor Z.
/// Compiles into a `PlannedWindow` — data the policy reads, not a forced cast.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CooldownAssignment {
    /// Roster index of the assigned player.
    pub player: u8,
    /// Ability name in that player's kit.
    pub ability: String,
    pub anchor: AnchorSpec,
}

/// Window anchor surface syntax.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnchorSpec {
    /// `anchor = "start"`, or `anchor = "grunt#1/warcry@2"` (a named enemy
    /// script occurrence).
    Named(String),
    /// `anchor = { offset_ms = 5000 }`: fixed offset from combat start.
    Offset { offset_ms: u64 },
}
