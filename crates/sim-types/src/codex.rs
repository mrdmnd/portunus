//! Compiled-actor runtime tables (SCAFFOLD.md §4, minimal shape).
//!
//! The full `sim-codex` crate (raw game data + annotations + provenance ->
//! `CompiledActor`) is not part of this scaffold; these are the flat tables
//! the kernel executes. Ability *effects* are opaque here: [`EffectRange`]
//! indexes into an effect-IR op arena owned by `sim-mechanics` (invariant 2 —
//! the kernel never sees game semantics, only schedulable shapes).

use serde::{Deserialize, Serialize};

use crate::ids::ResourceId;
use crate::time::SimTime;

/// A contiguous range of effect-IR ops in the mechanics op arena.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectRange {
    pub start: u32,
    pub len: u32,
}

/// Resource definition: analytic `(value, t0, rate)` parameters at reset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceDef {
    pub name: String,
    pub max: f64,
    pub initial: f64,
    /// Baseline regeneration per second (auras may multiply it).
    pub regen_per_sec: f64,
}

/// Cost gate on an ability.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceCost {
    pub resource: ResourceId,
    pub amount: f64,
}

/// One slot-indexed ability: the kernel reads the gates (cost, cooldown, cast
/// time, GCD flag); the mechanics layer interprets `effect` on completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbilityDef {
    pub name: String,
    pub cost: Option<ResourceCost>,
    /// 0 = no cooldown.
    pub cooldown: SimTime,
    /// 0 = instant.
    pub cast_time: SimTime,
    pub on_gcd: bool,
    /// Whether the ability requires an enemy target.
    pub targeted: bool,
    pub effect: EffectRange,
}

/// Periodic (tick-train) component of an aura.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickDef {
    pub period: SimTime,
    pub effect: EffectRange,
}

/// One aura definition, indexed by [`AuraId`] within the source actor's kit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuraDef {
    pub name: String,
    pub duration: SimTime,
    /// Multiplier applied to a resource's regen rate while active
    /// (e.g. `(energy, 1.5)` for a haste-like frenzy).
    pub rate_mult: Option<(ResourceId, f64)>,
    /// Periodic tick train (e.g. a bleed), if any.
    pub tick: Option<TickDef>,
}

/// Flat runtime tables for one actor. Produced (eventually) by the codex
/// compiler; produced today by the toy-codex loader in `sim-mechanics`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompiledActor {
    pub name: String,
    pub max_health: f64,
    /// Global cooldown length (haste scaling is post-scaffold).
    pub gcd: SimTime,
    pub resources: Vec<ResourceDef>,
    pub abilities: Vec<AbilityDef>,
    pub auras: Vec<AuraDef>,
}

// ---- enemies ---------------------------------------------------------------
//
// Enemies are codex citizens exactly like players: their kits are calibrated
// data versioned with the game build, never authored per run. A run file only
// *references* them by name. (Scaffold home: the bestiary loader lives in
// `sim-encounter` until a real `sim-codex` crate exists.)

use crate::dist::{HpSpec, JitteredMs};

/// One enemy definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyDef {
    pub name: String,
    pub hp: HpSpec,
    /// Enemy-forces contribution per mob (count ledger).
    pub count: u32,
    /// Scripted behavior, relative to this enemy's own engagement.
    pub script: Vec<EnemyScriptDef>,
}

/// One scripted ability line of an enemy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnemyScriptDef {
    /// Stable id — plans anchor cooldown assignments to `spawn/script@occ`.
    pub id: String,
    /// Delay from engagement to the first occurrence.
    pub first: JitteredMs,
    /// Repeat interval; `None` = fires once.
    pub every: Option<JitteredMs>,
    pub effect: ScriptEffect,
}

/// What a script occurrence does (closed vocabulary, grown additively like
/// the player-side effect IR).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ScriptEffect {
    /// Unavoidable damage.
    Intake { amount: f64, target: IntakeTargetSpec },
}

/// Intake targeting *distribution*; resolved to a concrete
/// [`crate::encounter::IntakeTarget`] per occurrence at sample time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntakeTargetSpec {
    AllPlayers,
    RandomPlayer,
}

/// The enemy codex: name-addressed enemy definitions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bestiary {
    /// Sorted by name for deterministic iteration.
    pub enemies: Vec<EnemyDef>,
}

impl Bestiary {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&EnemyDef> {
        self.enemies.iter().find(|e| e.name == name)
    }
}
