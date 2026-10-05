//! One concrete run: static random values drawn, names resolved to indices.

use portunus_core::{EnemyKey, PullName, Seed, SimDuration, SpawnLabel, Trigger};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRun {
    pub name: String,
    /// The seed this run was sampled from. The engine draws every dynamic
    /// value from it too, so it is the rollout's only seed.
    pub seed: Seed,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Segment {
    /// `prepull` is the trailing part of `duration` in which seats may act.
    Travel {
        duration: SimDuration,
        prepull: SimDuration,
    },
    Combat(ResolvedCombat),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedCombat {
    pub pull: PullName,
    pub timeout: SimDuration,
    /// Spawns known before combat. Adds from `SpawnAdds` exist only in
    /// engine state, labeled under their parent, e.g. `boss#1/add#3`.
    pub spawns: Vec<ResolvedSpawn>,
}

/// Index into [`ResolvedCombat::spawns`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SpawnIndex(pub u16);

/// Resolved subject for engagement triggers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnSet {
    One(SpawnIndex),
    Many(Vec<SpawnIndex>),
    Engaged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedSpawn {
    pub label: SpawnLabel,
    pub enemy: EnemyKey,
    pub max_health: f64,
    pub forces: u32,
    pub engage: Trigger<SpawnSet>,
}
