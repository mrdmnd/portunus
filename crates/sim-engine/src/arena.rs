//! Full simulation state (invariant 7, scaffold form).
//!
//! All mutable rollout state lives in one `SimState`, built from flat vectors
//! of mostly-`Copy` rows so `snapshot()` is a cheap structural clone. The
//! true single-contiguous-arena layout (snapshot == literal memcpy) is a
//! post-G0 optimization behind the same [`crate::Engine`] interface.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sim_types::{rng, ActorId, AuraId, Gen, Seed, SimTime, StreamId};

use crate::resources::AnalyticResource;

/// Whether an actor is a policy-driven player or a scripted enemy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorKind {
    /// Index into the roster of `CompiledActor`s.
    Player { roster_idx: usize },
    /// Enemies act only through scripted encounter events.
    Enemy,
}

/// One active-or-vacant aura slot on an actor. Slots are reused; every
/// (re)activation and deactivation bumps `gen`, lazily invalidating the old
/// expiry event and tick train (invariant 5).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AuraSlot {
    pub active: bool,
    /// Aura definition index within the *source* actor's kit.
    pub aura: AuraId,
    /// Who applied it (whose codex defines it, who gets tick credit).
    pub source: ActorId,
    pub expires_at: SimTime,
    pub stacks: u16,
    pub gen: Gen,
}

/// Per-actor mutable state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorState {
    pub kind: ActorKind,
    pub alive: bool,
    /// Enemies exist from reset but only enter combat when their engagement
    /// trigger fires; players are always engaged. Unengaged enemies are not
    /// targetable and their scripts are not running.
    pub engaged: bool,
    pub health: AnalyticResource,
    /// Indexed by `ResourceId`; empty for enemies.
    pub resources: Vec<AnalyticResource>,
    /// Absolute ready times, indexed by `SlotId`; empty for enemies.
    pub cooldown_ready: Vec<SimTime>,
    /// Absolute time the current global cooldown ends (may be in the past).
    pub gcd_ready: SimTime,
    /// True while a non-instant cast is in flight.
    pub casting: bool,
    pub auras: Vec<AuraSlot>,
    /// Actor generation; bumped on death to invalidate pending events.
    pub gen: Gen,
    /// Mask-watch generation; bumped whenever the engine reschedules the
    /// earliest mask-change crossing for this actor.
    pub mask_watch_gen: Gen,
    // -- outcome bookkeeping --
    pub damage_done: f64,
    pub casts: u32,
}

impl ActorState {
    /// Find an active aura slot by (definition, source).
    #[must_use]
    pub fn find_aura(&self, aura: AuraId, source: ActorId) -> Option<usize> {
        self.auras.iter().position(|s| s.active && s.aura == aura && s.source == source)
    }

    /// Find any active aura slot by definition id (any source).
    #[must_use]
    pub fn find_aura_any_source(&self, aura: AuraId) -> Option<usize> {
        self.auras.iter().position(|s| s.active && s.aura == aura)
    }

    /// Claim a slot for an aura instance: refresh the existing (aura, source)
    /// slot or reuse a vacant one or grow. Bumps the slot generation and
    /// returns `(slot_index, new_gen)` for scheduling guarded events.
    pub fn claim_aura_slot(&mut self, aura: AuraId, source: ActorId) -> (usize, Gen) {
        let idx = self
            .find_aura(aura, source)
            .or_else(|| self.auras.iter().position(|s| !s.active))
            .unwrap_or_else(|| {
                self.auras.push(AuraSlot {
                    active: false,
                    aura,
                    source,
                    expires_at: 0,
                    stacks: 0,
                    gen: Gen(0),
                });
                self.auras.len() - 1
            });
        let slot = &mut self.auras[idx];
        slot.aura = aura;
        slot.source = source;
        slot.gen.bump();
        (idx, slot.gen)
    }
}

/// The whole mutable simulation state for one rollout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub now: SimTime,
    pub seed: Seed,
    pub actors: Vec<ActorState>,
    /// Player deaths so far (outcome bookkeeping).
    pub deaths: u32,
    /// Per-(actor, stream) occurrence counters for the counter-based RNG
    /// (invariant 6). Part of state so snapshot/restore replays identically.
    /// BTreeMap keeps iteration deterministic.
    pub rng_occurrence: BTreeMap<(ActorId, StreamId), u64>,
}

impl SimState {
    /// Draw the next value from an actor's semantic stream, advancing the
    /// occurrence counter.
    pub fn rng_next(&mut self, actor: ActorId, stream: StreamId) -> u64 {
        let occ = self.rng_occurrence.entry((actor, stream)).or_insert(0);
        let x = rng::draw_u64(self.seed, rng::actor_stream_domain(actor, stream), *occ);
        *occ += 1;
        x
    }

    /// Actor ids of all living, *engaged* enemies (the targetable set),
    /// in id order.
    pub fn targetable_enemies(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.actors.iter().enumerate().filter_map(|(i, a)| {
            (a.alive && a.engaged && matches!(a.kind, ActorKind::Enemy))
                .then_some(ActorId(i as u8))
        })
    }

    /// Actor ids of all living players, in id order.
    pub fn alive_players(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.actors.iter().enumerate().filter_map(|(i, a)| {
            (a.alive && matches!(a.kind, ActorKind::Player { .. })).then_some(ActorId(i as u8))
        })
    }
}
