//! The authored route.
//!
//! A pull is one combat made of waves. Gathering, chain-pulling, timed
//! body-pulls, and dragging trash onto a boss are all the same construct:
//! a wave whose engagement trigger says when its enemies' clocks start.

use portunus_core::{Dist, EnemyKey, PullName, SimDuration, SpawnLabel, Trigger};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioSpec {
    pub name: String,
    /// Enemy-forces requirement, checked against everything pulled.
    pub forces_required: Option<u32>,
    pub pulls: Vec<PullSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullSpec {
    pub name: PullName,
    /// Downtime before this pull: travel, drinking, waiting on cooldowns.
    pub travel_in: Dist<SimDuration>,
    /// The last part of `travel_in` spent ready at the pack: seats may cast
    /// non-hostile spells (pre-pots, self-buffs, pre-cast stacks), and a
    /// cast still in progress at combat start carries over.
    pub prepull: SimDuration,
    /// Wipe deadline, relative to combat start.
    pub timeout: SimDuration,
    pub waves: Vec<WaveSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaveSpec {
    /// Instances are labeled `key#n`, counting across waves within the pull.
    pub mobs: Vec<(EnemyKey, u32)>,
    pub engage: Trigger<WaveSubject>,
}

/// What a wave's engagement trigger may refer to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveSubject {
    Spawn(SpawnLabel),
    /// Another wave in this pull, by index.
    Wave(usize),
    /// Everything engaged so far.
    Engaged,
}
