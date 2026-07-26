//! Small copyable identifier newtypes shared across every crate.

use serde::{Deserialize, Serialize};

/// Actor index: `0..PARTY_SIZE` are players, `PARTY_SIZE..` are enemies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ActorId(pub u8);

/// Codex-compiled ability slot index within one actor's kit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SlotId(pub u16);

/// Index into the enemy target list for explicit target selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TargetSlot(pub u8);

/// Generation counter for lazy event invalidation (invariant 5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Gen(pub u32);

impl Gen {
    /// Advance the generation, invalidating every event guarded by the old value.
    pub fn bump(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

/// Root RNG seed for one rollout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Seed(pub u64);

/// Handle to an engine-compiled predicate (see `sim-engine`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PredicateId(pub u16);

/// Codex-compiled resource index within one actor (energy, rage, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceId(pub u8);

/// Codex-compiled aura index within the *source* actor's kit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AuraId(pub u16);

/// Semantic RNG stream identifier (invariant 6): each stochastic mechanic
/// draws from its own stream so common-random-number pairing survives
/// reordering of draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StreamId(pub u16);

/// Game client build a codex was compiled against, e.g. `"11.2.5.61188"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameBuild(pub String);
