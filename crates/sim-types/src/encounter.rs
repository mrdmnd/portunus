//! The resolved run model: what a compiled "dungeon run" looks like.
//!
//! A run is a flat alternating sequence of travel and combat segments — no
//! route graph. Each combat is a set of *spawns* (individual mobs; packs are
//! authoring sugar) with an **engagement spec** each: the primitive that
//! expresses gathering, chaining, body-pulls, and pulling trash onto bosses
//! is simply *when each mob's clock starts*.
//!
//! Sampling discipline: everything random is already drawn by the time this
//! struct exists — HP rolls, script timing jitter, intake targets. A spawn's
//! script is stored as offsets **relative to its own engagement**; the engine
//! anchors them when the engagement trigger fires. Conditional engagement
//! times are a deterministic function of combat evolution, so the whole
//! rollout stays bit-reproducible (invariant 1).

use serde::{Deserialize, Serialize};

use crate::ids::ActorId;
use crate::time::SimTime;

/// A fully sampled, ready-to-execute dungeon run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRun {
    pub name: String,
    pub segments: Vec<RunSegment>,
}

impl ResolvedRun {
    /// Convenience: a run consisting of a single combat, no travel.
    #[must_use]
    pub fn single_combat(combat: ResolvedCombat) -> Self {
        Self { name: combat.name.clone(), segments: vec![RunSegment::Combat(combat)] }
    }

    /// Iterate the combat segments in order.
    pub fn combats(&self) -> impl Iterator<Item = &ResolvedCombat> {
        self.segments.iter().filter_map(|s| match s {
            RunSegment::Combat(c) => Some(c),
            RunSegment::Travel { .. } => None,
        })
    }
}

/// One step of the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RunSegment {
    /// Downtime between combats: travel, gathering, drinking. The sampled
    /// duration is concrete; the clock simply advances.
    Travel { duration: SimTime },
    /// One combat: everything that will enter it, and when.
    Combat(ResolvedCombat),
}

/// One combat = one pull (possibly a chained/merged one).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedCombat {
    /// Pull name from the run file (stable address for plans and traces).
    pub name: String,
    pub spawns: Vec<ResolvedSpawn>,
    /// Wipe deadline, relative to this combat's start.
    pub timeout: SimTime,
}

/// One concrete mob in a combat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedSpawn {
    /// Instance label unique within the combat, e.g. `grunt#2`.
    pub label: String,
    /// Bestiary/codex enemy id this spawn was built from.
    pub enemy: String,
    pub max_health: f64,
    /// Enemy-forces contribution (count ledger).
    pub count: u32,
    pub engage: EngageSpec,
    /// Pre-sampled script occurrences, offsets relative to this spawn's
    /// engagement, sorted by offset.
    pub script: Vec<ScriptedOccurrence>,
}

/// When a spawn enters combat.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EngageSpec {
    /// Part of the opening gather.
    AtStart,
    /// Timer relative to combat start (planned body-pull, patrol arrival).
    After(SimTime),
    /// Chained in when a condition over combat state becomes true.
    When(TriggerExpr),
}

/// Conditions over enemy-side combat state, evaluated by the engine (the
/// same lazily-armed-watcher machinery that serves player predicate waits).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TriggerExpr {
    /// Remaining HP of everything engaged so far, as a fraction of its
    /// summed max, drops below this.
    EngagedHpFracBelow(f64),
    /// At most this many engaged mobs remain alive.
    EngagedAliveAtMost(u32),
    /// A specific spawn (index within this combat) has died.
    SpawnDead(u32),
    /// Combat-relative time reaches this value.
    TimeAtLeast(SimTime),
}

/// One pre-sampled scripted event of a spawn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScriptedOccurrence {
    /// Offset from the spawn's engagement.
    pub offset: SimTime,
    pub kind: ScriptedEventKind,
}

/// What a scripted occurrence does. Damage intake is scripted profiles —
/// never emergent AI.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ScriptedEventKind {
    /// Unavoidable damage to one or all players.
    Intake { amount: f64, target: IntakeTarget },
}

/// Intake targeting, fully resolved at sample time (no in-engine randomness).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntakeTarget {
    AllPlayers,
    Player(ActorId),
}

/// What a "realistic" InfoSet substitutes for privileged timeline truth:
/// per-script mean schedules (the Projector fuzzes with these).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TimelinePriors {
    /// `(label, mean interval)` per stochastic script family,
    /// labels shaped `pull/spawn/script`.
    pub event_means: Vec<(String, SimTime)>,
}
