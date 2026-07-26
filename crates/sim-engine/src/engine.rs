//! The reference engine: the event-driven decision loop (SCAFFOLD.md §3).
//!
//! A rollout executes a whole [`ResolvedRun`]: travel segments advance the
//! clock (cooldowns and resources keep evolving analytically), combat
//! segments run the decision loop. Within a combat, every spawn has an
//! engagement spec — the opening gather (`AtStart`), timed body-pulls
//! (`After`), or chain conditions (`When`) evaluated by the same
//! lazily-armed-watcher machinery that serves player predicate waits.
//! Pulling trash onto a boss is nothing special: one combat, several waves.
//!
//! Responsibility split (decision-point completeness): the **engine owns
//! legality transitions** — it emits a decision point whenever an actor's
//! `ActionMask` would change (cooldown ready, GCD end, resource crossing a
//! masked ability's cost, a spawn engaging), auto-scheduling the earliest
//! such crossing so a bare `Wait(NextEvent)` is complete w.r.t. mask changes.
//! The **policy owns value transitions**, declared via wait predicates.
//!
//! Scaffold simplifications, documented not hidden: decision points exist
//! only while a combat is active (no travel-time decisions); dead players
//! revive at the next combat's start (death-count still recorded).

use serde::{Deserialize, Serialize};
use sim_types::{
    to_secs_f32, Action, ActionMask, ActorId, ActorOutcome, AuraId, CompiledActor, IntakeTarget,
    Outcome, Plan, PredicateExpr, PredicateId, ResolvedCombat, ResolvedRun, RunSegment,
    ScriptedEventKind, Seed, SimTime, SlotId, StreamId, TargetSel, TriggerExpr, WaitMenu,
    WaitSpec, MAX_ACTIONS,
};

use crate::arena::{ActorKind, ActorState, SimState};
use crate::error::EngineError;
use crate::event::{Event, EventPayload, GenGuard};
use crate::predicate::{self, PredicateRegistry, WakePlan};
use crate::priority::Priority;
use crate::resources::AnalyticResource;
use crate::scheduler::EventScheduler;

/// Why an actor was woken for a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WakeReason {
    /// A combat segment just began.
    CombatStart,
    GcdEnd,
    CastEnd,
    CooldownReady(SlotId),
    /// A resource crossed a masked ability's cost (engine auto-scheduled).
    ResourceCross,
    /// An enemy spawn entered combat (targets changed).
    SpawnEngaged,
    /// A proc or effect granted something decision-relevant.
    ProcApplied(AuraId),
    AuraExpired(AuraId),
    /// Policy-scheduled `Wait::Until` fired.
    WakeAt,
    /// Policy-scheduled predicate became true.
    PredicateMet(PredicateId),
}

impl WakeReason {
    /// A wake is *anticipated* iff the policy scheduled it itself; the
    /// Projector applies reaction latency only to non-anticipated wakes.
    #[must_use]
    pub fn anticipated(self) -> bool {
        matches!(self, WakeReason::WakeAt | WakeReason::PredicateMet(_))
    }
}

/// A pending decision handed to exactly one policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionPoint {
    pub actor: ActorId,
    pub trigger: WakeReason,
    pub anticipated: bool,
}

/// What `next_decision` yields.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Decide(DecisionPoint),
    Done(Outcome),
}

/// Everything a rollout needs, borrowed immutably for its duration.
#[derive(Debug, Clone, Copy)]
pub struct RolloutContext<'a> {
    pub run: &'a ResolvedRun,
    pub roster: &'a [CompiledActor],
    pub plan: &'a Plan,
    pub seed: Seed,
}

/// Kernel services handed to the mechanics layer while it interprets effect
/// IR. Mechanics mutates state and schedules events only through this.
pub struct EngineIo<'a> {
    pub state: &'a mut SimState,
    pub sched: &'a mut EventScheduler,
    pub roster: &'a [CompiledActor],
    /// Wake requests collected during event application; the kernel turns
    /// them into decision points (deduplicated per actor).
    pub wakes: &'a mut Vec<(ActorId, WakeReason)>,
}

impl EngineIo<'_> {
    /// Current sim time.
    #[must_use]
    pub fn now(&self) -> SimTime {
        self.state.now
    }

    /// Deal a damage lump, with attribution and death bookkeeping.
    pub fn deal_damage(&mut self, source: ActorId, target: ActorId, amount: f64) {
        let now = self.state.now;
        let t = &mut self.state.actors[usize::from(target.0)];
        if !t.alive {
            return;
        }
        t.health.add(now, -amount);
        if t.health.value_at(now) <= 0.0 {
            t.alive = false;
            t.gen.bump(); // lazily invalidate every event scheduled against this actor
            if matches!(t.kind, ActorKind::Player { .. }) {
                self.state.deaths += 1;
            }
        }
        self.state.actors[usize::from(source.0)].damage_done += amount;
    }

    /// Draw a uniform in `[0,1)` from an actor's semantic stream.
    pub fn rng_uniform(&mut self, actor: ActorId, stream: StreamId) -> f64 {
        sim_types::rng::uniform01(self.state.rng_next(actor, stream))
    }

    /// Ask the kernel to emit a decision point for `actor` after this event.
    pub fn request_wake(&mut self, actor: ActorId, reason: WakeReason) {
        self.wakes.push((actor, reason));
    }
}

/// Game semantics socket (invariant 2). The kernel calls these hooks for
/// every game-semantic event; `sim-mechanics` implements them by interpreting
/// effect IR. Implementations are stateless — all persistence lives in
/// `SimState` (statelessness is an invariant, not a style).
pub trait Mechanics {
    fn cast_complete(
        &self,
        io: &mut EngineIo<'_>,
        actor: ActorId,
        slot: SlotId,
        target: ActorId,
    ) -> Result<(), EngineError>;

    fn periodic_tick(
        &self,
        io: &mut EngineIo<'_>,
        holder: ActorId,
        aura_slot: usize,
    ) -> Result<(), EngineError>;

    fn aura_expired(
        &self,
        io: &mut EngineIo<'_>,
        holder: ActorId,
        aura_slot: usize,
    ) -> Result<(), EngineError>;
}

/// No-op mechanics for kernel-only tests: casts complete, nothing happens.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMechanics;

impl Mechanics for NoopMechanics {
    fn cast_complete(
        &self,
        _io: &mut EngineIo<'_>,
        _actor: ActorId,
        _slot: SlotId,
        _target: ActorId,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn periodic_tick(
        &self,
        _io: &mut EngineIo<'_>,
        _holder: ActorId,
        _aura_slot: usize,
    ) -> Result<(), EngineError> {
        Ok(())
    }

    fn aura_expired(
        &self,
        io: &mut EngineIo<'_>,
        holder: ActorId,
        aura_slot: usize,
    ) -> Result<(), EngineError> {
        let slot = &mut io.state.actors[usize::from(holder.0)].auras[aura_slot];
        slot.active = false;
        slot.gen.bump();
        Ok(())
    }
}

/// The engine contract (SCAFFOLD.md §3).
pub trait Engine<'a> {
    fn reset(&mut self, cx: RolloutContext<'a>) -> Result<(), EngineError>;
    /// Run the event loop until some actor must decide (or the run ends).
    /// Idempotent while a decision is outstanding: re-emits the same point.
    fn next_decision(&mut self) -> Step;
    fn legal(&self, actor: ActorId) -> ActionMask;
    fn submit(&mut self, actor: ActorId, action: Action) -> Result<(), EngineError>;
    fn now(&self) -> SimTime;
    fn snapshot(&self) -> Snapshot;
    fn restore(&mut self, s: &Snapshot);
    /// For the Projector and tracing only — policies never see this
    /// (invariant 8).
    fn state(&self) -> &SimState;
    /// Compile a predicate for use in `Wait::UntilPredicate`.
    fn compile_predicate(&mut self, expr: &PredicateExpr) -> PredicateId;
}

/// Runtime bookkeeping for one combat segment.
#[derive(Debug, Clone)]
struct CombatRt {
    /// Index into `run.segments`.
    segment: usize,
    /// First actor index of this combat's spawns.
    actor_start: usize,
    actor_len: usize,
    /// Set when the combat begins.
    start_time: SimTime,
    begun: bool,
    /// Actor indices with un-fired `When` engagement conditions.
    pending_when: Vec<usize>,
    /// Count of un-fired `After` timers (for the never-fires fallback rule).
    pending_after: u32,
}

/// Cloneable core: everything `snapshot`/`restore` must capture, including
/// the pending-event queue (invariant 7).
#[derive(Debug, Clone)]
struct EngineCore {
    state: SimState,
    sched: EventScheduler,
    /// Decisions produced by already-applied events, not yet handed out.
    /// `(decision, trigger_seq)`.
    pending: Vec<(DecisionPoint, u64)>,
    /// Decision handed out and awaiting `submit`.
    awaiting: Option<(DecisionPoint, u64)>,
    finished: Option<Outcome>,
    /// Armed subscription-backend predicate waits: `(actor, predicate)`.
    armed_predicates: Vec<(ActorId, PredicateId)>,
    /// Per-player: trigger seq of the last bare `Wait(NextEvent)`, for
    /// livelock detection.
    last_bare_wait: Vec<Option<u64>>,
    /// Per-player: `(timestamp, decisions consumed at that timestamp)`.
    /// A policy that keeps waiting without time advancing is livelocked.
    decision_spin: Vec<(SimTime, u32)>,
    /// Combat segments in run order.
    combats: Vec<CombatRt>,
    /// Index into `combats` of the currently active combat, if any.
    active_combat: Option<usize>,
    predicates: PredicateRegistry,
    /// Order-sensitive fold over applied events; equal hashes across two
    /// rollouts mean identical event traces (determinism CI).
    trace_hash: u64,
    /// Monotone counter distinguishing wake-request "events" (procs) from
    /// heap events for livelock accounting.
    wake_seq: u64,
}

/// Opaque snapshot of a rollout in flight. O(state size) to take and restore.
#[derive(Debug, Clone)]
pub struct Snapshot(EngineCore);

/// The CPU reference engine, generic over the mechanics implementation.
pub struct ReferenceEngine<'a, M: Mechanics> {
    mechanics: M,
    cx: Option<RolloutContext<'a>>,
    core: EngineCore,
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv_fold(h: u64, x: u64) -> u64 {
    let mut h = h;
    for b in x.to_le_bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(FNV_PRIME);
    }
    h
}

impl<'a, M: Mechanics> ReferenceEngine<'a, M> {
    #[must_use]
    pub fn new(mechanics: M) -> Self {
        Self {
            mechanics,
            cx: None,
            core: EngineCore {
                state: SimState {
                    now: 0,
                    seed: Seed(0),
                    actors: Vec::new(),
                    deaths: 0,
                    rng_occurrence: std::collections::BTreeMap::new(),
                },
                sched: EventScheduler::new(),
                pending: Vec::new(),
                awaiting: None,
                finished: None,
                armed_predicates: Vec::new(),
                last_bare_wait: Vec::new(),
                decision_spin: Vec::new(),
                combats: Vec::new(),
                active_combat: None,
                predicates: PredicateRegistry::default(),
                trace_hash: FNV_OFFSET,
                wake_seq: u64::MAX / 2,
            },
        }
    }

    /// Order-sensitive hash of every event applied so far. Two rollouts with
    /// equal hashes processed identical event sequences.
    #[must_use]
    pub fn trace_hash(&self) -> u64 {
        self.core.trace_hash
    }

    fn cx(&self) -> &RolloutContext<'a> {
        self.cx.as_ref().expect("engine used before reset")
    }

    fn actor(&self, id: ActorId) -> &ActorState {
        &self.core.state.actors[usize::from(id.0)]
    }

    fn roster_entry(&self, id: ActorId) -> Option<&'a CompiledActor> {
        match self.actor(id).kind {
            ActorKind::Player { roster_idx } => Some(&self.cx().roster[roster_idx]),
            ActorKind::Enemy => None,
        }
    }

    /// The combat data for `combats[idx]`.
    fn combat_data(&self, idx: usize) -> &'a ResolvedCombat {
        match &self.cx().run.segments[self.core.combats[idx].segment] {
            RunSegment::Combat(c) => c,
            RunSegment::Travel { .. } => unreachable!("combat index maps to combat segment"),
        }
    }

    /// Is this event still valid under its generation guard?
    fn guard_valid(state: &SimState, guard: Option<GenGuard>) -> bool {
        match guard {
            None => true,
            Some(GenGuard::Actor(a, gen)) => state.actors[usize::from(a.0)].gen == gen,
            Some(GenGuard::Aura { holder, slot, gen }) => state.actors[usize::from(holder.0)]
                .auras
                .get(slot)
                .is_some_and(|s| s.gen == gen),
            Some(GenGuard::MaskWatch(a, gen)) => {
                state.actors[usize::from(a.0)].mask_watch_gen == gen
            }
        }
    }

    fn push_wake(&mut self, actor: ActorId, reason: WakeReason, trigger_seq: u64) {
        // Decisions exist only while a combat is active (scaffold rule).
        if self.core.active_combat.is_none() {
            return;
        }
        let a = self.actor(actor);
        if !a.alive || !matches!(a.kind, ActorKind::Player { .. }) || a.casting {
            return;
        }
        // One outstanding decision per actor at a time keeps the loop sane;
        // coalesced triggers are indistinguishable to a mask-driven policy.
        let already = self.core.pending.iter().any(|(dp, _)| dp.actor == actor)
            || self.core.awaiting.as_ref().is_some_and(|(dp, _)| dp.actor == actor);
        if already {
            return;
        }
        self.core.pending.push((
            DecisionPoint { actor, trigger: reason, anticipated: reason.anticipated() },
            trigger_seq,
        ));
    }

    fn wake_all_players(&mut self, reason: WakeReason, trigger_seq: u64) {
        for i in 0..self.cx().roster.len() {
            self.push_wake(ActorId(i as u8), reason, trigger_seq);
        }
    }

    /// Engage a spawn: its clock starts now — schedule its pre-sampled
    /// script relative to this instant.
    fn engage_spawn(&mut self, combat_idx: usize, actor_idx: usize, trigger_seq: u64) {
        let now = self.core.state.now;
        let combat = self.combat_data(combat_idx);
        let spawn_idx = actor_idx - self.core.combats[combat_idx].actor_start;
        let spawn = &combat.spawns[spawn_idx];
        let actor = ActorId(actor_idx as u8);

        let a = &mut self.core.state.actors[actor_idx];
        if !a.alive || a.engaged {
            return;
        }
        a.engaged = true;
        let gen = a.gen;

        for (occ, s) in spawn.script.iter().enumerate() {
            self.core.sched.schedule(
                now + s.offset,
                Priority::Encounter,
                EventPayload::EnemyScript { actor, occurrence: occ as u32 },
                Some(GenGuard::Actor(actor, gen)),
            );
        }
        // Targets changed: a legality transition for every player.
        self.wake_all_players(WakeReason::SpawnEngaged, trigger_seq);
    }

    /// Begin a combat segment: revive stragglers, fire the opening gather,
    /// arm timers and chain conditions, start the timeout clock.
    fn begin_combat(&mut self, combat_idx: usize, trigger_seq: u64) {
        let now = self.core.state.now;
        self.core.active_combat = Some(combat_idx);

        // Scaffold rule: dead players rejoin at the next pull (deaths remain
        // in the outcome). Reschedule their still-running cooldown wakes,
        // which death's gen bump lazily invalidated.
        for i in 0..self.cx().roster.len() {
            let ca_max = self.cx().roster[i].max_health;
            let a = &mut self.core.state.actors[i];
            if !a.alive {
                a.alive = true;
                a.casting = false;
                a.health = AnalyticResource::new(ca_max, now, 0.0, 0.0, ca_max);
                a.mask_watch_gen.bump();
                let gen = a.gen;
                let ready: Vec<(usize, SimTime)> = a
                    .cooldown_ready
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|&(_, t)| t > now)
                    .collect();
                let gcd_ready = a.gcd_ready;
                for (slot, t) in ready {
                    self.core.sched.schedule(
                        t,
                        Priority::LegalityChange,
                        EventPayload::CooldownReady {
                            actor: ActorId(i as u8),
                            slot: SlotId(slot as u16),
                        },
                        Some(GenGuard::Actor(ActorId(i as u8), gen)),
                    );
                }
                if gcd_ready > now {
                    self.core.sched.schedule(
                        gcd_ready,
                        Priority::LegalityChange,
                        EventPayload::GcdEnd { actor: ActorId(i as u8) },
                        Some(GenGuard::Actor(ActorId(i as u8), gen)),
                    );
                }
            }
        }

        let combat = self.combat_data(combat_idx);
        let (actor_start, actor_len) = {
            let rt = &self.core.combats[combat_idx];
            (rt.actor_start, rt.actor_len)
        };
        debug_assert_eq!(actor_len, combat.spawns.len());

        let mut pending_when = Vec::new();
        let mut pending_after = 0_u32;
        let mut opening = Vec::new();
        for (si, spawn) in combat.spawns.iter().enumerate() {
            let actor_idx = actor_start + si;
            match spawn.engage {
                sim_types::EngageSpec::AtStart => opening.push(actor_idx),
                sim_types::EngageSpec::After(dt) => {
                    pending_after += 1;
                    let actor = ActorId(actor_idx as u8);
                    let gen = self.core.state.actors[actor_idx].gen;
                    self.core.sched.schedule(
                        now + dt,
                        Priority::Encounter,
                        EventPayload::EngageSpawn { actor },
                        Some(GenGuard::Actor(actor, gen)),
                    );
                }
                sim_types::EngageSpec::When(_) => pending_when.push(actor_idx),
            }
        }
        {
            let rt = &mut self.core.combats[combat_idx];
            rt.begun = true;
            rt.start_time = now;
            rt.pending_when = pending_when;
            rt.pending_after = pending_after;
        }

        self.core.sched.schedule(
            now + combat.timeout,
            Priority::Bookkeeping,
            EventPayload::CombatTimeout { combat: combat_idx as u32 },
            None,
        );

        // Wake with CombatStart before opening engagements so the pull's
        // first decision is attributed to the combat, not a spawn.
        self.wake_all_players(WakeReason::CombatStart, trigger_seq);
        for actor_idx in opening {
            self.engage_spawn(combat_idx, actor_idx, trigger_seq);
        }
    }

    /// Apply one event.
    fn apply_event(&mut self, ev: Event) -> Result<(), EngineError> {
        // Fold the event into the determinism trace hash.
        let h = self.core.trace_hash;
        let h = fnv_fold(h, ev.time);
        let h = fnv_fold(h, ev.priority as u64);
        let h = fnv_fold(h, payload_tag(&ev.payload));
        self.core.trace_hash = h;

        let mut wakes: Vec<(ActorId, WakeReason)> = Vec::new();
        let cx = *self.cx();

        match ev.payload {
            EventPayload::CastComplete { actor, slot, target } => {
                self.core.state.actors[usize::from(actor.0)].casting = false;
                let mut io = EngineIo {
                    state: &mut self.core.state,
                    sched: &mut self.core.sched,
                    roster: cx.roster,
                    wakes: &mut wakes,
                };
                self.mechanics.cast_complete(&mut io, actor, slot, target)?;
                self.push_wake(actor, WakeReason::CastEnd, ev.seq);
            }
            EventPayload::GcdEnd { actor } => {
                self.push_wake(actor, WakeReason::GcdEnd, ev.seq);
            }
            EventPayload::CooldownReady { actor, slot } => {
                self.push_wake(actor, WakeReason::CooldownReady(slot), ev.seq);
            }
            EventPayload::ResourceCross { actor } => {
                self.push_wake(actor, WakeReason::ResourceCross, ev.seq);
            }
            EventPayload::WakeAt { actor } => {
                self.push_wake(actor, WakeReason::WakeAt, ev.seq);
            }
            EventPayload::WakePredicate { actor, pred } => {
                // Re-check at fire: a rate mutation since scheduling may have
                // moved the crossing. If no longer true, fall back to the
                // subscription backend rather than waking spuriously.
                let expr = self
                    .core
                    .predicates
                    .get(pred)
                    .ok_or(EngineError::UnknownPredicate(pred))?
                    .clone();
                if predicate::eval(&self.core.state, actor, &expr) {
                    self.push_wake(actor, WakeReason::PredicateMet(pred), ev.seq);
                } else {
                    self.core.armed_predicates.push((actor, pred));
                }
            }
            EventPayload::AuraExpire { holder, slot } => {
                let aura = self.core.state.actors[usize::from(holder.0)].auras[slot].aura;
                let mut io = EngineIo {
                    state: &mut self.core.state,
                    sched: &mut self.core.sched,
                    roster: cx.roster,
                    wakes: &mut wakes,
                };
                self.mechanics.aura_expired(&mut io, holder, slot)?;
                self.push_wake(holder, WakeReason::AuraExpired(aura), ev.seq);
            }
            EventPayload::PeriodicTick { holder, slot } => {
                let mut io = EngineIo {
                    state: &mut self.core.state,
                    sched: &mut self.core.sched,
                    roster: cx.roster,
                    wakes: &mut wakes,
                };
                self.mechanics.periodic_tick(&mut io, holder, slot)?;
            }
            EventPayload::CombatBegin { combat } => {
                self.begin_combat(combat as usize, ev.seq);
            }
            EventPayload::EngageSpawn { actor } => {
                if let Some(c) = self.core.active_combat {
                    self.core.combats[c].pending_after =
                        self.core.combats[c].pending_after.saturating_sub(1);
                    self.engage_spawn(c, usize::from(actor.0), ev.seq);
                }
            }
            EventPayload::EnemyScript { actor, occurrence } => {
                self.apply_enemy_script(actor, occurrence);
            }
            EventPayload::CombatTimeout { combat } => {
                if self.core.active_combat == Some(combat as usize) {
                    self.finish(None);
                }
            }
        }

        // Mechanics-requested wakes (procs etc.) count as fresh triggers.
        for (actor, reason) in wakes {
            self.core.wake_seq += 1;
            let seq = self.core.wake_seq;
            self.push_wake(actor, reason, seq);
        }
        Ok(())
    }

    fn apply_enemy_script(&mut self, actor: ActorId, occurrence: u32) {
        let Some(c) = self.core.active_combat else { return };
        let src = &self.core.state.actors[usize::from(actor.0)];
        if !src.alive || !src.engaged {
            return;
        }
        let combat = self.combat_data(c);
        let spawn_idx = usize::from(actor.0) - self.core.combats[c].actor_start;
        let Some(occ) = combat.spawns[spawn_idx].script.get(occurrence as usize) else {
            return;
        };
        match occ.kind {
            ScriptedEventKind::Intake { amount, target } => {
                let targets: Vec<ActorId> = match target {
                    IntakeTarget::AllPlayers => self.core.state.alive_players().collect(),
                    IntakeTarget::Player(p) => vec![p],
                };
                for t in targets {
                    let now = self.core.state.now;
                    // Sampled targets assume a full party; smaller rosters
                    // (tests) simply shed the excess intake.
                    let Some(a) = self.core.state.actors.get_mut(usize::from(t.0)) else {
                        continue;
                    };
                    if !a.alive || !matches!(a.kind, ActorKind::Player { .. }) {
                        continue;
                    }
                    a.health.add(now, -amount);
                    if a.health.value_at(now) <= 0.0 {
                        a.alive = false;
                        a.gen.bump();
                        self.core.state.deaths += 1;
                    }
                    self.core.state.actors[usize::from(actor.0)].damage_done += amount;
                }
            }
        }
    }

    /// End the run and record the outcome.
    fn finish(&mut self, kill_time: Option<SimTime>) {
        let state = &self.core.state;
        let per_actor = state
            .actors
            .iter()
            .enumerate()
            .filter(|(_, a)| matches!(a.kind, ActorKind::Player { .. }))
            .map(|(i, a)| ActorOutcome {
                actor: ActorId(i as u8),
                damage_done: a.damage_done,
                casts: a.casts,
                died: !a.alive,
            })
            .collect();
        self.core.finished =
            Some(Outcome { kill_time, deaths: state.deaths, per_actor });
        self.core.pending.clear();
        self.core.awaiting = None;
        self.core.active_combat = None;
    }

    /// Evaluate a chain condition against current combat state.
    fn trigger_met(&self, combat_idx: usize, expr: TriggerExpr) -> bool {
        let rt = &self.core.combats[combat_idx];
        let state = &self.core.state;
        let now = state.now;
        let engaged =
            || (rt.actor_start..rt.actor_start + rt.actor_len).filter(|&i| state.actors[i].engaged);
        match expr {
            TriggerExpr::EngagedHpFracBelow(f) => {
                let (mut hp, mut max) = (0.0_f64, 0.0_f64);
                for i in engaged() {
                    let a = &state.actors[i];
                    max += a.health.max();
                    if a.alive {
                        hp += a.health.value_at(now);
                    }
                }
                max > 0.0 && hp / max < f
            }
            TriggerExpr::EngagedAliveAtMost(n) => {
                engaged().filter(|&i| state.actors[i].alive).count() as u32 <= n
            }
            TriggerExpr::SpawnDead(si) => state
                .actors
                .get(rt.actor_start + si as usize)
                .is_some_and(|a| !a.alive),
            TriggerExpr::TimeAtLeast(t) => now.saturating_sub(rt.start_time) >= t,
        }
    }

    /// After every event: fire chain conditions (to fixpoint), apply the
    /// never-fires fallback, then handle combat completion / run transition.
    fn combat_progress(&mut self, trigger_seq: u64) {
        if self.core.finished.is_some() {
            return;
        }
        // Everyone dead is a wipe regardless of combat state.
        if self.core.state.alive_players().next().is_none() {
            self.finish(None);
            return;
        }
        let Some(c) = self.core.active_combat else { return };

        // Fire chain conditions until a fixpoint (an engagement can only
        // raise engaged HP / alive counts, so this terminates fast).
        loop {
            let rt = &self.core.combats[c];
            let combat = self.combat_data(c);
            let fired: Vec<usize> = rt
                .pending_when
                .iter()
                .copied()
                .filter(|&actor_idx| {
                    let si = actor_idx - rt.actor_start;
                    match combat.spawns[si].engage {
                        sim_types::EngageSpec::When(expr) => self.trigger_met(c, expr),
                        _ => false,
                    }
                })
                .collect();
            if fired.is_empty() {
                break;
            }
            self.core.combats[c].pending_when.retain(|a| !fired.contains(a));
            for actor_idx in fired {
                self.engage_spawn(c, actor_idx, trigger_seq);
            }
        }

        // Fallback (semantic decision, documented): if everything engaged is
        // dead but conditional spawns remain and no timers are coming, the
        // team walks over and pulls them — engage all pending now.
        {
            let rt = &self.core.combats[c];
            let any_engaged_alive = (rt.actor_start..rt.actor_start + rt.actor_len)
                .any(|i| self.core.state.actors[i].engaged && self.core.state.actors[i].alive);
            if !any_engaged_alive && rt.pending_after == 0 && !rt.pending_when.is_empty() {
                let pending = std::mem::take(&mut self.core.combats[c].pending_when);
                for actor_idx in pending {
                    self.engage_spawn(c, actor_idx, trigger_seq);
                }
            }
        }

        // Completion: every spawn of this combat is dead.
        let rt = &self.core.combats[c];
        let all_dead = (rt.actor_start..rt.actor_start + rt.actor_len)
            .all(|i| !self.core.state.actors[i].alive);
        if !all_dead {
            return;
        }
        self.core.active_combat = None;

        // Advance through travel to the next combat, or finish the run.
        let now = self.core.state.now;
        if let Some(next_idx) = self.core.combats.iter().position(|rt| !rt.begun) {
            let segments = &self.cx().run.segments;
            let prev_segment = self.core.combats[c].segment;
            let next_segment = self.core.combats[next_idx].segment;
            let travel: SimTime = segments[prev_segment + 1..next_segment]
                .iter()
                .map(|s| match s {
                    RunSegment::Travel { duration } => *duration,
                    RunSegment::Combat(_) => 0,
                })
                .sum();
            self.core.sched.schedule(
                now + travel,
                Priority::LegalityChange,
                EventPayload::CombatBegin { combat: next_idx as u32 },
                None,
            );
        } else {
            // No more combats: the run is complete at the last kill.
            self.finish(Some(now));
        }
    }

    /// Recheck armed subscription predicates after an event (the
    /// event-subscription backend).
    fn recheck_armed_predicates(&mut self) {
        if self.core.armed_predicates.is_empty() {
            return;
        }
        let armed = std::mem::take(&mut self.core.armed_predicates);
        for (actor, pred) in armed {
            let Some(expr) = self.core.predicates.get(pred).cloned() else { continue };
            if self.actor(actor).alive && predicate::eval(&self.core.state, actor, &expr) {
                self.core.wake_seq += 1;
                let seq = self.core.wake_seq;
                self.push_wake(actor, WakeReason::PredicateMet(pred), seq);
            } else {
                self.core.armed_predicates.push((actor, pred));
            }
        }
    }

    /// Absolute time at which `slot` becomes castable for `actor`, or `None`
    /// if unreachable under current rates. `now` if castable immediately.
    fn usable_at(&self, actor: ActorId, slot_idx: usize) -> Option<SimTime> {
        let a = self.actor(actor);
        let ca = self.roster_entry(actor)?;
        let ability = ca.abilities.get(slot_idx)?;
        let now = self.core.state.now;
        let mut t = now;
        t = t.max(a.cooldown_ready.get(slot_idx).copied().unwrap_or(0));
        if ability.on_gcd {
            t = t.max(a.gcd_ready);
        }
        if let Some(cost) = ability.cost {
            let res = a.resources.get(usize::from(cost.resource.0))?;
            t = t.max(res.time_to_reach(now, cost.amount)?);
        }
        Some(t)
    }

    /// Auto-schedule the earliest mask-change resource crossing for a waiting
    /// actor (cooldown/GCD transitions already have their own events).
    fn schedule_mask_watch(&mut self, actor: ActorId) {
        let Some(ca) = self.roster_entry(actor) else { return };
        let now = self.core.state.now;
        let a = &self.core.state.actors[usize::from(actor.0)];

        let mut earliest: Option<SimTime> = None;
        for (i, ability) in ca.abilities.iter().enumerate() {
            let Some(cost) = ability.cost else { continue };
            let Some(res) = a.resources.get(usize::from(cost.resource.0)) else { continue };
            let Some(t) = res.time_to_reach(now, cost.amount) else { continue };
            // Only future crossings change the mask.
            let ready = t
                .max(a.cooldown_ready.get(i).copied().unwrap_or(0))
                .max(if ability.on_gcd { a.gcd_ready } else { 0 });
            if ready > now {
                earliest = Some(earliest.map_or(ready, |e| e.min(ready)));
            }
        }

        let a = &mut self.core.state.actors[usize::from(actor.0)];
        a.mask_watch_gen.bump(); // invalidate any previous watcher
        if let Some(t) = earliest {
            self.core.sched.schedule(
                t,
                Priority::LegalityChange,
                EventPayload::ResourceCross { actor },
                Some(GenGuard::MaskWatch(actor, a.mask_watch_gen)),
            );
        }
    }

    fn resolve_target(&self, actor: ActorId, sel: TargetSel) -> Result<ActorId, EngineError> {
        let state = &self.core.state;
        let illegal = |reason: &str| EngineError::IllegalAction {
            actor,
            reason: reason.to_string(),
        };
        match sel {
            TargetSel::Primary => {
                state.targetable_enemies().next().ok_or_else(|| illegal("no targetable enemy"))
            }
            TargetSel::Slot(k) => state
                .targetable_enemies()
                .nth(usize::from(k.0))
                .ok_or_else(|| illegal("target slot out of range")),
            TargetSel::LowestHp => state
                .targetable_enemies()
                .min_by(|x, y| {
                    let hx = state.actors[usize::from(x.0)].health.value_at(state.now);
                    let hy = state.actors[usize::from(y.0)].health.value_at(state.now);
                    hx.total_cmp(&hy).then(x.cmp(y))
                })
                .ok_or_else(|| illegal("no targetable enemy")),
            TargetSel::InterruptTarget => {
                // Enemy cast bars land with the interrupt machinery (post-G0).
                Err(illegal("no interruptible enemy cast"))
            }
        }
    }

    fn submit_cast(
        &mut self,
        actor: ActorId,
        slot: SlotId,
        sel: TargetSel,
    ) -> Result<(), EngineError> {
        let mask = self.legal(actor);
        if !mask.is_castable(slot) {
            return Err(EngineError::IllegalAction {
                actor,
                reason: format!("slot {} not castable now", slot.0),
            });
        }
        let ca = self.roster_entry(actor).ok_or(EngineError::UnknownSlot { actor, slot })?;
        let ability = ca
            .abilities
            .get(usize::from(slot.0))
            .ok_or(EngineError::UnknownSlot { actor, slot })?;
        let target = if ability.targeted {
            self.resolve_target(actor, sel)?
        } else {
            actor
        };

        let now = self.core.state.now;
        let gcd = ca.gcd;
        let (cost, cooldown, cast_time, on_gcd) =
            (ability.cost, ability.cooldown, ability.cast_time, ability.on_gcd);
        let actor_gen = self.actor(actor).gen;

        let a = &mut self.core.state.actors[usize::from(actor.0)];
        if let Some(c) = cost {
            a.resources[usize::from(c.resource.0)].add(now, -c.amount);
        }
        if on_gcd {
            a.gcd_ready = now + gcd;
        }
        if cooldown > 0 {
            a.cooldown_ready[usize::from(slot.0)] = now + cooldown;
        }
        a.casting = cast_time > 0;
        a.casts += 1;

        if on_gcd {
            self.core.sched.schedule(
                now + gcd,
                Priority::LegalityChange,
                EventPayload::GcdEnd { actor },
                Some(GenGuard::Actor(actor, actor_gen)),
            );
        }
        if cooldown > 0 {
            self.core.sched.schedule(
                now + cooldown,
                Priority::LegalityChange,
                EventPayload::CooldownReady { actor, slot },
                Some(GenGuard::Actor(actor, actor_gen)),
            );
        }
        // Instant casts complete via a zero-delay event: events are the only
        // mutation sites (invariant 3), even at the same timestamp.
        self.core.sched.schedule(
            now + cast_time,
            Priority::CastComplete,
            EventPayload::CastComplete { actor, slot, target },
            Some(GenGuard::Actor(actor, actor_gen)),
        );
        self.core.last_bare_wait[usize::from(actor.0)] = None;
        Ok(())
    }

    fn submit_wait(
        &mut self,
        actor: ActorId,
        spec: WaitSpec,
        trigger_seq: u64,
    ) -> Result<(), EngineError> {
        let now = self.core.state.now;
        let actor_gen = self.actor(actor).gen;
        // Livelock rule (§3): re-invocation at the same timestamp requires a
        // new event, and each wait must make progress possible. A policy that
        // keeps consuming same-instant decisions with waits is spinning.
        const SPIN_LIMIT: u32 = 64;
        if self.core.decision_spin[usize::from(actor.0)].1 > SPIN_LIMIT {
            return Err(EngineError::Livelock(actor));
        }
        match spec {
            WaitSpec::NextEvent => {
                // Livelock rule (§3): a decision consumes its triggering
                // event; bare-waiting twice on the same trigger means no new
                // event could ever re-wake this actor differently.
                let last = &mut self.core.last_bare_wait[usize::from(actor.0)];
                if *last == Some(trigger_seq) {
                    return Err(EngineError::Livelock(actor));
                }
                *last = Some(trigger_seq);
            }
            WaitSpec::Until(t) => {
                if t <= now {
                    return Err(EngineError::IllegalAction {
                        actor,
                        reason: format!("Wait::Until({t}) is not in the future (now={now})"),
                    });
                }
                self.core.sched.schedule(
                    t,
                    Priority::ScheduledWake,
                    EventPayload::WakeAt { actor },
                    Some(GenGuard::Actor(actor, actor_gen)),
                );
                self.core.last_bare_wait[usize::from(actor.0)] = None;
            }
            WaitSpec::UntilPredicate(pred) => {
                let expr = self
                    .core
                    .predicates
                    .get(pred)
                    .ok_or(EngineError::UnknownPredicate(pred))?
                    .clone();
                if predicate::eval(&self.core.state, actor, &expr) {
                    // Already true: wake via a fresh same-timestamp event
                    // (legal — new event, new trigger).
                    self.core.sched.schedule(
                        now,
                        Priority::ScheduledWake,
                        EventPayload::WakePredicate { actor, pred },
                        Some(GenGuard::Actor(actor, actor_gen)),
                    );
                } else {
                    match predicate::plan_wake(&self.core.state, actor, self.cx().roster, &expr) {
                        WakePlan::At(t) => {
                            self.core.sched.schedule(
                                t.max(now),
                                Priority::ScheduledWake,
                                EventPayload::WakePredicate { actor, pred },
                                Some(GenGuard::Actor(actor, actor_gen)),
                            );
                        }
                        WakePlan::Subscribe => {
                            self.core.armed_predicates.push((actor, pred));
                        }
                    }
                }
                self.core.last_bare_wait[usize::from(actor.0)] = None;
            }
            WaitSpec::GcdEnd => {
                if self.actor(actor).gcd_ready <= now {
                    return Err(EngineError::IllegalAction {
                        actor,
                        reason: "Wait::GcdEnd with no GCD running".to_string(),
                    });
                }
                // The GcdEnd event was scheduled when the GCD started.
                self.core.last_bare_wait[usize::from(actor.0)] = None;
            }
        }
        // Every wait is an interruptible lease: make sure the earliest
        // mask-changing resource crossing has a wake event (completeness).
        self.schedule_mask_watch(actor);
        Ok(())
    }
}

fn payload_tag(p: &EventPayload) -> u64 {
    // Compact, stable content tag for trace hashing.
    match *p {
        EventPayload::CastComplete { actor, slot, .. } => {
            0x0100 | u64::from(actor.0) | (u64::from(slot.0) << 16)
        }
        EventPayload::GcdEnd { actor } => 0x0200 | u64::from(actor.0),
        EventPayload::CooldownReady { actor, slot } => {
            0x0300 | u64::from(actor.0) | (u64::from(slot.0) << 16)
        }
        EventPayload::ResourceCross { actor } => 0x0400 | u64::from(actor.0),
        EventPayload::WakeAt { actor } => 0x0500 | u64::from(actor.0),
        EventPayload::WakePredicate { actor, pred } => {
            0x0600 | u64::from(actor.0) | (u64::from(pred.0) << 16)
        }
        EventPayload::AuraExpire { holder, slot } => {
            0x0700 | u64::from(holder.0) | ((slot as u64) << 16)
        }
        EventPayload::PeriodicTick { holder, slot } => {
            0x0800 | u64::from(holder.0) | ((slot as u64) << 16)
        }
        EventPayload::CombatBegin { combat } => 0x0900 | (u64::from(combat) << 16),
        EventPayload::EngageSpawn { actor } => 0x0a00 | u64::from(actor.0),
        EventPayload::EnemyScript { actor, occurrence } => {
            0x0b00 | u64::from(actor.0) | (u64::from(occurrence) << 16)
        }
        EventPayload::CombatTimeout { combat } => 0x0c00 | (u64::from(combat) << 16),
    }
}

impl<'a, M: Mechanics> Engine<'a> for ReferenceEngine<'a, M> {
    fn reset(&mut self, cx: RolloutContext<'a>) -> Result<(), EngineError> {
        let mut actors: Vec<ActorState> = Vec::new();

        for (roster_idx, ca) in cx.roster.iter().enumerate() {
            actors.push(ActorState {
                kind: ActorKind::Player { roster_idx },
                alive: true,
                engaged: true,
                health: AnalyticResource::new(ca.max_health, 0, 0.0, 0.0, ca.max_health),
                resources: ca
                    .resources
                    .iter()
                    .map(|r| AnalyticResource::new(r.initial, 0, r.regen_per_sec, 0.0, r.max))
                    .collect(),
                cooldown_ready: vec![0; ca.abilities.len()],
                gcd_ready: 0,
                casting: false,
                auras: Vec::new(),
                gen: sim_types::Gen(0),
                mask_watch_gen: sim_types::Gen(0),
                damage_done: 0.0,
                casts: 0,
            });
        }

        // All combats' spawns exist as actors from reset (deterministic actor
        // indexing); they engage when their trigger fires.
        let mut combats: Vec<CombatRt> = Vec::new();
        for (seg_idx, seg) in cx.run.segments.iter().enumerate() {
            let RunSegment::Combat(combat) = seg else { continue };
            let actor_start = actors.len();
            for spawn in &combat.spawns {
                actors.push(ActorState {
                    kind: ActorKind::Enemy,
                    alive: true,
                    engaged: false,
                    health: AnalyticResource::new(
                        spawn.max_health,
                        0,
                        0.0,
                        0.0,
                        spawn.max_health,
                    ),
                    resources: Vec::new(),
                    cooldown_ready: Vec::new(),
                    gcd_ready: 0,
                    casting: false,
                    auras: Vec::new(),
                    gen: sim_types::Gen(0),
                    mask_watch_gen: sim_types::Gen(0),
                    damage_done: 0.0,
                    casts: 0,
                });
            }
            combats.push(CombatRt {
                segment: seg_idx,
                actor_start,
                actor_len: combat.spawns.len(),
                start_time: 0,
                begun: false,
                pending_when: Vec::new(),
                pending_after: 0,
            });
        }

        self.core.state = SimState {
            now: 0,
            seed: cx.seed,
            actors,
            deaths: 0,
            rng_occurrence: std::collections::BTreeMap::new(),
        };
        self.core.sched = EventScheduler::new();
        self.core.pending.clear();
        self.core.awaiting = None;
        self.core.finished = None;
        self.core.armed_predicates.clear();
        self.core.last_bare_wait = vec![None; cx.roster.len()];
        self.core.decision_spin = vec![(0, 0); cx.roster.len()];
        self.core.combats = combats;
        self.core.active_combat = None;
        self.core.trace_hash = FNV_OFFSET;
        self.core.wake_seq = u64::MAX / 2;

        // Schedule the first combat after any leading travel; a run with no
        // combats is trivially complete.
        if self.core.combats.is_empty() {
            self.finish(Some(0));
        } else {
            let first_seg = self.core.combats[0].segment;
            let lead_travel: SimTime = cx.run.segments[..first_seg]
                .iter()
                .map(|s| match s {
                    RunSegment::Travel { duration } => *duration,
                    RunSegment::Combat(_) => 0,
                })
                .sum();
            self.core.sched.schedule(
                lead_travel,
                Priority::LegalityChange,
                EventPayload::CombatBegin { combat: 0 },
                None,
            );
        }

        self.cx = Some(cx);
        Ok(())
    }

    fn next_decision(&mut self) -> Step {
        if let Some(o) = &self.core.finished {
            return Step::Done(o.clone());
        }
        if let Some((dp, _)) = &self.core.awaiting {
            return Step::Decide(*dp); // idempotent until submit
        }
        loop {
            while let Some((dp, seq)) = self.core.pending.first().copied() {
                self.core.pending.remove(0);
                if !self.actor(dp.actor).alive {
                    continue; // died after the wake was queued (same-tick intake)
                }
                let spin = &mut self.core.decision_spin[usize::from(dp.actor.0)];
                if spin.0 == self.core.state.now {
                    spin.1 += 1;
                } else {
                    *spin = (self.core.state.now, 1);
                }
                self.core.awaiting = Some((dp, seq));
                return Step::Decide(dp);
            }
            let Some(ev) = self.core.sched.pop() else {
                // Queue exhausted with the run unfinished and no one waking:
                // nothing can ever change again — end as a wipe.
                self.finish(None);
                return Step::Done(self.core.finished.clone().expect("just finished"));
            };
            if !Self::guard_valid(&self.core.state, ev.guard) {
                continue; // lazy invalidation (invariant 5)
            }
            debug_assert!(ev.time >= self.core.state.now, "time went backwards");
            self.core.state.now = ev.time;
            let seq = ev.seq;
            if let Err(e) = self.apply_event(ev) {
                // Mechanics errors abort the rollout as a wipe; the trace
                // hash still identifies the divergent path.
                debug_assert!(false, "mechanics error mid-rollout: {e}");
                self.finish(None);
                return Step::Done(self.core.finished.clone().expect("just finished"));
            }
            self.combat_progress(seq);
            if let Some(o) = &self.core.finished {
                return Step::Done(o.clone());
            }
            self.recheck_armed_predicates();
        }
    }

    fn legal(&self, actor: ActorId) -> ActionMask {
        let a = self.actor(actor);
        let Some(ca) = self.roster_entry(actor) else {
            return ActionMask::none();
        };
        if !a.alive || a.casting {
            return ActionMask::none();
        }
        let now = self.core.state.now;
        let mut mask = ActionMask::none();
        for i in 0..ca.abilities.len().min(MAX_ACTIONS) {
            match self.usable_at(actor, i) {
                Some(t) if t <= now => {
                    mask.castable_now |= 1_u64 << i;
                    mask.usable_in[i] = 0.0;
                }
                Some(t) => mask.usable_in[i] = to_secs_f32(t - now),
                None => mask.usable_in[i] = f32::INFINITY,
            }
        }
        mask.wait_menu = WaitMenu {
            gcd_end: (a.gcd_ready > now).then_some(a.gcd_ready),
            next_cooldown_ready: a
                .cooldown_ready
                .iter()
                .copied()
                .filter(|&t| t > now)
                .min(),
            next_scheduled_event: self.core.sched.peek_time(),
        };
        mask
    }

    fn submit(&mut self, actor: ActorId, action: Action) -> Result<(), EngineError> {
        let (dp, trigger_seq) = self
            .core
            .awaiting
            .ok_or(EngineError::NoPendingDecision(actor))?;
        if dp.actor != actor {
            return Err(EngineError::WrongActor { expected: dp.actor, got: actor });
        }
        let result = match action {
            Action::Cast { slot, target } => self.submit_cast(actor, slot, target),
            Action::Wait(spec) => self.submit_wait(actor, spec, trigger_seq),
        };
        if result.is_ok() {
            self.core.awaiting = None;
        }
        result
    }

    fn now(&self) -> SimTime {
        self.core.state.now
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot(self.core.clone())
    }

    fn restore(&mut self, s: &Snapshot) {
        self.core = s.0.clone();
    }

    fn state(&self) -> &SimState {
        &self.core.state
    }

    fn compile_predicate(&mut self, expr: &PredicateExpr) -> PredicateId {
        self.core.predicates.compile(expr)
    }
}
