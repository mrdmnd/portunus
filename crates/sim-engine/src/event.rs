//! Event payloads and generation guards.
//!
//! Four event families (SCAFFOLD.md §3): actor-scheduled, aura-scheduled,
//! encounter-scheduled, and bookkeeping. Every schedulable entity carries a
//! generation; stale events are discarded at pop (invariant 5) — there is no
//! surgical heap deletion.

use serde::{Deserialize, Serialize};
use sim_types::{ActorId, Gen, PredicateId, SimTime, SlotId};

/// Generation guard attached to an event at schedule time. The event is
/// dropped at pop if the referenced generation has moved on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenGuard {
    /// Valid while the actor's generation is unchanged (bumped on death).
    Actor(ActorId, Gen),
    /// Valid while the aura slot's generation is unchanged (bumped on
    /// refresh, reapply, expiry — invalidating old expiries and tick trains).
    Aura { holder: ActorId, slot: usize, gen: Gen },
    /// Valid while the actor's mask-watch generation is unchanged (bumped
    /// whenever the engine recomputes the earliest mask-change crossing).
    MaskWatch(ActorId, Gen),
}

/// What happens when an event fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPayload {
    // -- actor-scheduled --------------------------------------------------
    /// A cast finishes; mechanics interprets the ability's effect IR.
    CastComplete { actor: ActorId, slot: SlotId, target: ActorId },
    /// The actor's global cooldown ends.
    GcdEnd { actor: ActorId },
    /// An ability cooldown becomes ready (a legality transition).
    CooldownReady { actor: ActorId, slot: SlotId },
    /// Auto-scheduled earliest resource crossing that changes the actor's
    /// `ActionMask` (the engine owns legality transitions, §3).
    ResourceCross { actor: ActorId },
    /// Policy-requested `Wait::Until` wake.
    WakeAt { actor: ActorId },
    /// Policy-requested predicate wake (analytically solved crossing).
    WakePredicate { actor: ActorId, pred: PredicateId },

    // -- aura-scheduled ---------------------------------------------------
    /// An aura instance expires.
    AuraExpire { holder: ActorId, slot: usize },
    /// One tick of an aura's periodic train.
    PeriodicTick { holder: ActorId, slot: usize },

    // -- encounter-scheduled ----------------------------------------------
    /// A combat segment of the run begins (after travel).
    CombatBegin { combat: u32 },
    /// A timed engagement fires (planned body-pull, patrol arrival).
    EngageSpawn { actor: ActorId },
    /// One pre-sampled scripted occurrence of an engaged enemy.
    EnemyScript { actor: ActorId, occurrence: u32 },

    // -- bookkeeping --------------------------------------------------------
    /// Combat timeout: the run ends as a wipe if this combat is still going.
    CombatTimeout { combat: u32 },
}

/// A scheduled event: ordering key fields plus payload and optional guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub time: SimTime,
    pub priority: crate::priority::Priority,
    /// Monotone tie-breaker assigned at schedule time; makes ordering total.
    pub seq: u64,
    pub payload: EventPayload,
    pub guard: Option<GenGuard>,
}
