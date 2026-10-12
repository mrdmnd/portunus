//! Everything one rollout starts from.

use std::sync::Arc;

use portunus_core::{AuraId, Dist, SimDuration};
pub use portunus_gamedata::class::PullAura;
use portunus_gamedata::{EnemyData, GameData};
use portunus_loadout::ActorTemplate;
use portunus_scenario::ResolvedRun;
use serde::{Deserialize, Serialize};

/// The plan is deliberately absent: it is advice for policies, delivered
/// through observations, and has no effect on the world.
///
/// Party members whose roles aren't being optimized (tank, healer) are
/// ordinary seats with simple stand-in loadouts driven by scripted
/// policies, not a separate engine concept.
#[derive(Debug, Clone)]
pub struct RunSetup {
    /// Carries the rollout's seed.
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
/// advance, so these are short (SimC's `queue_lag`, 5 ms). Unanticipated
/// wakes (procs, enemy casts, engagements) cost a human reaction time,
/// drawn separately for each event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Latency {
    pub anticipated: Dist<SimDuration>,
    pub reaction: Dist<SimDuration>,
    /// Network delay before a cast chosen on an unanticipated wake starts
    /// (SimC's `world_lag`, 100 ms). The seat is committed meanwhile, and
    /// the cast is checked again when the lag runs out. Casts chosen on
    /// anticipated wakes were queued ahead, which hides it.
    #[serde(default = "no_lag")]
    pub cast_lag: Dist<SimDuration>,
}

fn no_lag() -> Dist<SimDuration> {
    Dist::Fixed(SimDuration::ZERO)
}

/// Buffs and debuffs from raiders outside the simulated seats. The seats'
/// own classes add theirs ([`portunus_gamedata::class::ClassDef`]); list
/// here only what the rest of a group would bring.
///
/// External auras use their holder as source.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Externals {
    /// Applied to every seat at run start.
    #[serde(default)]
    pub party_auras: Vec<AuraId>,
    /// Applied to every enemy when it engages.
    #[serde(default)]
    pub enemy_auras: Vec<AuraId>,
    /// Applied to every living seat as each pull starts.
    #[serde(default)]
    pub on_pull: Vec<PullAura>,
    /// Who decides when the seats' classes' Bloodlust goes out.
    #[serde(default)]
    pub bloodlust: BloodlustMode,
    /// The group's shared battle resurrection charges.
    #[serde(default)]
    pub battle_res: BattleRes,
}

/// Battle resurrection charges, shared by the whole group and counted from
/// the start of the run. A resurrection taken in combat (`Effect::Resurrect`,
/// a Soulstone) spends one when the dead seat accepts it. Charges recharge
/// one at a time while below `max`, like a spell's.
///
/// The defaults are Mythic+'s: one charge at the start of the dungeon,
/// another every 10 minutes, at most 5 stored. SimC has no resurrections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BattleRes {
    pub initial: u8,
    pub every: SimDuration,
    pub max: u8,
}

impl Default for BattleRes {
    fn default() -> Self {
        BattleRes {
            initial: 1,
            every: SimDuration::from_secs(600),
            max: 5,
        }
    }
}

/// When a class's on-pull aura with a granting spell ([`PullAura::spell`])
/// goes out. Auras from `Externals::on_pull`, and classes without the
/// spell, are always automatic.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BloodlustMode {
    /// On every living seat as each pull starts.
    #[default]
    Auto,
    /// Each seat of a providing class has the spell, and casts it when its
    /// policy chooses.
    Policy,
}
