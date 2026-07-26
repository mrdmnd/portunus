//! Shared plain types for the Portunus simulation workspace.
//!
//! Everything in this crate is plain data: serde-serializable, no heavy
//! dependencies, no behavior beyond small pure helpers. Every other crate in
//! the workspace depends on this one; this crate depends on nothing but
//! `serde` (dependency rule, SCAFFOLD.md §1).

pub mod action;
pub mod codex;
pub mod dist;
pub mod encounter;
pub mod ids;
pub mod outcome;
pub mod plan;
pub mod predicate;
pub mod rng;
pub mod time;

pub use action::{Action, ActionMask, TargetSel, WaitMenu, WaitSpec, MAX_ACTIONS};
pub use codex::{
    AbilityDef, AuraDef, Bestiary, CompiledActor, EffectRange, EnemyDef, EnemyScriptDef,
    IntakeTargetSpec, ResourceCost, ResourceDef, ScriptEffect, TickDef,
};
pub use dist::{HpSpec, JitteredMs};
pub use encounter::{
    EngageSpec, IntakeTarget, ResolvedCombat, ResolvedRun, ResolvedSpawn, RunSegment,
    ScriptedEventKind, ScriptedOccurrence, TimelinePriors, TriggerExpr,
};
pub use ids::{ActorId, AuraId, GameBuild, Gen, PredicateId, ResourceId, Seed, SlotId, StreamId, TargetSlot};
pub use outcome::{ActorOutcome, Outcome};
pub use plan::{Plan, PlanConstraints, PlannedWindow, PullPlan, WindowAnchor};
pub use predicate::{CmpOp, PredicateExpr, ScalarRef};
pub use time::{from_millis, from_secs_f64, to_secs_f32, SimTime, MICROS_PER_MILLI, MICROS_PER_SEC};

/// Fixed party size for the five-player team the whole system targets.
pub const PARTY_SIZE: usize = 5;
