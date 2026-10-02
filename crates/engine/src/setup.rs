//! Everything one rollout starts from.

use std::sync::Arc;

use portunus_core::{AuraId, Dist, Seed, SimDuration};
use portunus_gamedata::{EnemyData, GameData};
use portunus_loadout::ActorTemplate;
use portunus_scenario::ResolvedRun;

/// The plan is deliberately absent: it is advice for policies, delivered
/// through observations, and has no effect on the world.
///
/// Party members whose roles aren't being optimized (tank, healer) are
/// ordinary seats with simple stand-in templates driven by scripted
/// policies, not a separate engine concept.
#[derive(Debug, Clone)]
pub struct RunSetup {
    pub seed: Seed,
    pub run: Arc<ResolvedRun>,
    pub data: Arc<GameData>,
    pub enemies: Arc<EnemyData>,
    /// Index is the seat.
    pub seats: Vec<SeatSetup>,
    pub externals: Externals,
    pub record_trace: bool,
}

#[derive(Debug, Clone)]
pub struct SeatSetup {
    pub template: ActorTemplate,
    pub latency: Latency,
}

/// How quickly a seat responds, drawn per wake from the seat's own random
/// stream.
///
/// Anticipated wakes (a wait the seat named, its own cast or GCD ending)
/// model the spell queue window: a real player lines the next cast up in
/// advance, so these default to zero delay. Unanticipated wakes (procs,
/// enemy casts, engagements) cost a human reaction time, drawn separately
/// for each event.
#[derive(Debug, Clone, PartialEq)]
pub struct Latency {
    pub anticipated: Dist<SimDuration>,
    pub reaction: Dist<SimDuration>,
    /// Network delay between submitting a cast and it starting.
    pub cast_lag: Dist<SimDuration>,
}

/// Permanent buffs and debuffs from outside the simulated seats, such as raid
/// buffs a full group would bring. Timed cooldowns (Bloodlust, Power
/// Infusion) are never externals: some seat casts them, guided by the plan,
/// and Exhaustion-style lockouts are `AuraDef::blocked_by`.
///
/// External auras use their holder as source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Externals {
    /// Applied to every seat at run start.
    pub party_auras: Vec<AuraId>,
    /// Applied to every enemy when it engages.
    pub enemy_auras: Vec<AuraId>,
}
