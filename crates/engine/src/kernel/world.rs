//! Rollout state: everything that changes, plus a shared handle on what
//! doesn't. Cloning a [`World`] is the fork.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use portunus_core::rng::{self, Domain, Purpose};
use portunus_core::{ActorId, AuraId, Sample, Seat, SimDuration, SimTime, SpellId, StreamKey};
use portunus_gamedata::effect::{ModKind, ModScope, Predicate};
use portunus_gamedata::enemy::{PhaseName, RuleIndex};
use portunus_gamedata::item::{WeaponDef, WeaponHand};
use portunus_gamedata::pet::PetKind;
use portunus_gamedata::spell::{CastKind, GcdDef, SpellDef};
use portunus_gamedata::stats::{ResourceDef, ResourceKind};

use crate::choice::{Choice, Wait};
use crate::error::EngineError;
use crate::mechanics::{AuraChange, AuraEvent, CastEvent, DeathEvent};
use crate::outcome::{Outcome, PullOutcome, SeatOutcome};
use crate::setup::RunSetup;
use crate::state::{
    ActorKind, ActorView, AuraInstance, CastView, CastWhat, CombatView, CooldownView, DeckView,
    LastCast, PendingPerception, PendingTimer, ProcView, Projectile, ResourceView, RuleView,
    SeatPhase, SegmentView, StateView, SwingView,
};
use crate::step::{DecisionRequest, WakeReason};
use crate::trace::{TraceEvent, TraceRecord};

use super::queue::{Event, Queue, Wake};

/// What never changes during a rollout, shared by every fork.
pub(crate) struct Statics {
    pub setup: RunSetup,
    /// Per seat: `[anticipated, reaction]` latency domains.
    pub latency: Vec<[Domain; 2]>,
    /// Per seat, in a fixed order.
    pub abilities: Vec<Vec<SpellId>>,
    /// Per seat: the longest GCD among its abilities, for `gcd_length`.
    pub gcd: Vec<Option<GcdDef>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seg {
    Travel {
        segment: usize,
        combat: u16,
        prepull_from: SimTime,
        combat_starts: SimTime,
    },
    Combat {
        segment: usize,
        combat: u16,
        started: SimTime,
        deadline: SimTime,
    },
    Finished,
}

#[derive(Debug, Clone)]
pub(crate) struct Actor {
    pub kind: ActorKind,
    /// Stable name for random-stream domains.
    pub name: Arc<str>,
    pub health: u64,
    pub max_health: u64,
    pub alive: bool,
    pub engaged: bool,
    pub casting: Option<Casting>,
    pub cast_seq: u32,
    pub target: Option<ActorId>,
    /// Speed multiplier: 1.2 is 20% haste.
    pub haste: f64,
    pub attack_speed: f64,
    pub resources: Vec<Resource>,
    pub cooldowns: BTreeMap<SpellId, Cooldown>,
    pub auras: Vec<AuraInstance>,
    /// Parallel to `auras`.
    pub meta: Vec<AuraMeta>,
    pub procs: Vec<ProcView>,
    pub phase: Option<PhaseName>,
    /// Set for pets, guardians, and totems.
    pub pet: Option<PetLife>,
    /// Auto-attack timers: `[main hand, off hand]`.
    pub swings: [Option<Swing>; 2],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Swing {
    pub weapon: WeaponDef,
    /// `None` while not swinging (out of combat, or nothing to hit).
    pub next_at: Option<SimTime>,
    pub gen: u32,
}

impl Swing {
    pub fn new(weapon: WeaponDef) -> Self {
        Self {
            weapon,
            next_at: None,
            gen: 0,
        }
    }

    /// The hasted interval at a combined speed multiplier.
    pub fn interval(&self, speed: f64) -> SimDuration {
        millis_round(f64::from(self.weapon.speed.millis()) / speed.max(f64::MIN_POSITIVE))
            .max(SimDuration(1))
    }
}

impl Actor {
    /// Health left as a fraction of maximum; 0 with no maximum.
    pub fn health_frac(&self) -> f64 {
        if self.max_health == 0 {
            0.0
        } else {
            self.health as f64 / self.max_health as f64
        }
    }

    /// A fresh actor with no resources, auras, or target.
    pub fn new(kind: ActorKind, name: Arc<str>, max_health: u64, haste: f64) -> Self {
        Self {
            kind,
            name,
            health: max_health,
            max_health,
            alive: true,
            engaged: false,
            casting: None,
            cast_seq: 0,
            target: None,
            haste,
            attack_speed: 1.0,
            resources: Vec::new(),
            cooldowns: BTreeMap::new(),
            auras: Vec::new(),
            meta: Vec::new(),
            procs: Vec::new(),
            phase: None,
            pet: None,
            swings: [None, None],
        }
    }

    /// Haste times attack speed, which is what swing intervals divide by.
    pub fn swing_speed(&self) -> f64 {
        self.haste * self.attack_speed
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PetLife {
    pub kind: PetKind,
    pub expires: Option<SimTime>,
    /// Pets keep their own GCD.
    pub gcd_end: Option<SimTime>,
    /// Bumped whenever a pending autocast check becomes obsolete.
    pub gen: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Casting {
    pub ev: CastEvent,
    pub ends: SimTime,
    pub seq: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuraMeta {
    pub uid: u32,
    pub tick: Option<Tick>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Tick {
    pub last_at: SimTime,
    pub period: SimDuration,
    pub next_at: Option<SimTime>,
    pub index: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Resource {
    pub def: ResourceDef,
    pub value: f64,
    pub at: SimTime,
    pub regen_mult: f64,
}

impl Resource {
    /// Per second.
    pub fn rate(&self, haste: f64) -> f64 {
        let hasted = if self.def.regen_hasted { haste } else { 1.0 };
        self.def.regen_per_sec * self.regen_mult * hasted
    }

    pub fn value_at(&self, t: SimTime, haste: f64) -> f64 {
        let secs = f64::from(t.saturating_since(self.at).millis()) / 1000.0;
        (self.value + self.rate(haste) * secs).clamp(0.0, self.def.max)
    }

    pub fn settle(&mut self, now: SimTime, haste: f64) {
        self.value = self.value_at(now, haste);
        self.at = now;
    }
}

/// Recharge progress is kept in unhasted milliseconds of `base`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cooldown {
    pub charges: u8,
    pub max: u8,
    pub base: f64,
    pub hasted: bool,
    pub rate_mult: f64,
    pub progress: f64,
    pub at: SimTime,
    pub gen: u32,
}

impl Cooldown {
    /// Base milliseconds recovered per real millisecond.
    pub fn rate(&self, haste: f64) -> f64 {
        let hasted = if self.hasted { haste } else { 1.0 };
        (hasted * self.rate_mult).max(f64::MIN_POSITIVE)
    }

    pub fn settle(&mut self, now: SimTime, haste: f64) {
        if self.charges < self.max {
            self.progress += f64::from(now.saturating_since(self.at).millis()) * self.rate(haste);
        }
        self.at = now;
    }

    pub fn ready_at(&self, haste: f64) -> Option<SimTime> {
        (self.charges < self.max).then(|| {
            let left = (self.base - self.progress).max(0.0) / self.rate(haste);
            self.at + millis_ceil(left)
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SeatState {
    pub phase: SeatPhase,
    pub wait: Option<Wait>,
    /// Bumped whenever the seat's pending wake becomes obsolete.
    pub gen: u32,
    /// Where an armed seat's next wake is predicted.
    pub armed: Option<(SimTime, WakeReason)>,
    pub gcd_end: Option<SimTime>,
    pub last_cast: Option<LastCast>,
    pub unperceived: Vec<PendingPerception>,
    /// Parallel to `unperceived`.
    pub perception_ids: Vec<u32>,
    /// The last wait chosen and when, for livelock detection.
    pub waited: Option<(Wait, SimTime)>,
    /// Latency draws so far: `[anticipated, reaction]`.
    pub draws: [u64; 2],
    /// Pets summoned so far, for naming their random streams.
    pub summons: u32,
    pub outcome: SeatOutcome,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Followup {
    Changed(AuraChange),
    Removed(AuraEvent),
    Died(DeathEvent),
}

#[derive(Debug, Clone)]
pub(crate) struct Batch {
    pub requests: Vec<DecisionRequest>,
    pub answers: Vec<Option<Choice>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Trace {
    pub hash: u64,
    pub records: Option<Vec<TraceRecord>>,
    buf: Vec<u8>,
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

impl Trace {
    pub fn new(record: bool) -> Self {
        Self {
            hash: FNV_OFFSET,
            records: record.then(Vec::new),
            buf: Vec::new(),
        }
    }

    fn absorb(&mut self, rec: &TraceRecord) {
        let mut buf = std::mem::take(&mut self.buf);
        buf.clear();
        if let Ok(bytes) = postcard::to_extend(rec, buf) {
            for &b in &bytes {
                self.hash ^= u64::from(b);
                self.hash = self.hash.wrapping_mul(FNV_PRIME);
            }
            self.buf = bytes;
        }
    }
}

/// The full, privileged state of one rollout.
#[derive(Clone)]
pub struct World {
    pub(crate) s: Arc<Statics>,
    pub(crate) now: SimTime,
    pub(crate) seg: Seg,
    pub(crate) actors: Vec<Actor>,
    pub(crate) seat_actors: Vec<ActorId>,
    pub(crate) seats: Vec<SeatState>,
    /// Per seat: live pets, oldest first.
    pub(crate) pets: Vec<Vec<ActorId>>,
    pub(crate) enemies: Vec<ActorId>,
    pub(crate) projectiles: Vec<Projectile>,
    /// Parallel to `projectiles`.
    pub(crate) flights: Vec<(u32, CastEvent)>,
    pub(crate) timers: Vec<PendingTimer>,
    /// Parallel to `timers`.
    pub(crate) timer_ids: Vec<u32>,
    pub(crate) streams: BTreeMap<(ActorId, StreamKey), (Domain, u64)>,
    /// Deck-of-cards draws by `(holder, aura, listener index)`, kept across
    /// reapplications.
    pub(crate) decks: BTreeMap<(ActorId, AuraId, u8), DeckView>,
    pub(crate) queue: Queue,
    pub(crate) followups: VecDeque<Followup>,
    pub(crate) batch: Option<Batch>,
    pub(crate) pulls: Vec<PullOutcome>,
    pub(crate) next_id: u32,
    pub(crate) triggers_queued: Option<SimTime>,
    pub(crate) fault: Option<EngineError>,
    pub(crate) trace: Trace,
    pub(crate) finished: Option<Outcome>,
}

pub(crate) fn millis_ceil(ms: f64) -> SimDuration {
    SimDuration(ms.ceil().clamp(0.0, f64::from(u32::MAX / 2)) as u32)
}

pub(crate) fn hand_index(hand: WeaponHand) -> usize {
    match hand {
        WeaponHand::MainHand => 0,
        WeaponHand::OffHand => 1,
    }
}

pub(crate) fn millis_round(ms: f64) -> SimDuration {
    SimDuration(ms.round().clamp(0.0, f64::from(u32::MAX / 2)) as u32)
}

pub(crate) fn gcd_length(g: &GcdDef, haste: f64) -> SimDuration {
    if g.hasted {
        millis_round(f64::from(g.base.millis()) / haste).max(g.floor)
    } else {
        g.base
    }
}

fn scope_matches(scope: ModScope, spell: SpellId, def: Option<&SpellDef>) -> bool {
    match scope {
        ModScope::All => true,
        ModScope::Spell(s) => s == spell,
        ModScope::School(mask) => def.is_some_and(|d| d.school.0 & mask.0 != 0),
    }
}

impl World {
    pub(crate) fn fresh_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    pub(crate) fn fail(&mut self, e: EngineError) {
        if self.fault.is_none() {
            self.fault = Some(e);
        }
    }

    pub(crate) fn unsupported(&mut self, what: &str) {
        self.fail(EngineError::Unsupported(what.to_owned()));
    }

    pub(crate) fn record(&mut self, event: TraceEvent) {
        let rec = TraceRecord {
            time: self.now,
            event,
        };
        self.trace.absorb(&rec);
        if let Some(records) = &mut self.trace.records {
            records.push(rec);
        }
    }

    pub(crate) fn actor_ref(&self, id: ActorId) -> Option<&Actor> {
        self.actors.get(usize::from(id.0))
    }

    pub(crate) fn actor_mut(&mut self, id: ActorId) -> Option<&mut Actor> {
        self.actors.get_mut(usize::from(id.0))
    }

    pub(crate) fn seat_ref(&self, seat: Seat) -> Option<&SeatState> {
        self.seats.get(usize::from(seat.0))
    }

    pub(crate) fn seat_mut(&mut self, seat: Seat) -> &mut SeatState {
        &mut self.seats[usize::from(seat.0)]
    }

    pub(crate) fn seat_actor(&self, seat: Seat) -> ActorId {
        self.seat_actors[usize::from(seat.0)]
    }

    /// The seat behind a player or pet.
    pub(crate) fn owner_seat(&self, actor: ActorId) -> Option<Seat> {
        match self.actor_ref(actor)?.kind {
            ActorKind::Player(seat) | ActorKind::Pet { owner: seat, .. } => Some(seat),
            ActorKind::Enemy { .. } => None,
        }
    }

    pub(crate) fn player_seat(&self, actor: ActorId) -> Option<Seat> {
        match self.actor_ref(actor)?.kind {
            ActorKind::Player(seat) => Some(seat),
            _ => None,
        }
    }

    pub(crate) fn in_combat(&self) -> bool {
        matches!(self.seg, Seg::Combat { .. })
    }

    pub(crate) fn combat_index(&self) -> Option<u16> {
        match self.seg {
            Seg::Combat { combat, .. } => Some(combat),
            _ => None,
        }
    }

    pub(crate) fn live_targets(&self) -> Vec<ActorId> {
        self.enemies
            .iter()
            .copied()
            .filter(|&e| self.actor_ref(e).is_some_and(|a| a.alive && a.engaged))
            .collect()
    }

    pub(crate) fn is_live_target(&self, id: ActorId) -> bool {
        self.enemies.contains(&id) && self.actor_ref(id).is_some_and(|a| a.alive && a.engaged)
    }

    pub(crate) fn queue_triggers(&mut self) {
        if self.in_combat() && self.triggers_queued != Some(self.now) {
            self.triggers_queued = Some(self.now);
            self.queue.push(self.now, Event::EvalTriggers);
        }
    }

    /// Schedule a wake after the seat's latency draw. Idle and dead seats
    /// aren't told anything.
    pub(crate) fn notify(
        &mut self,
        seat: Seat,
        reason: WakeReason,
        anticipated: bool,
        gate: Option<u32>,
        event_at: SimTime,
    ) {
        let i = usize::from(seat.0);
        if matches!(self.seats[i].phase, SeatPhase::Idle | SeatPhase::Dead) {
            return;
        }
        let kind = usize::from(!anticipated);
        let latency = &self.s.setup.seats[i].latency;
        let dist = if anticipated {
            &latency.anticipated
        } else {
            &latency.reaction
        };
        let index = self.seats[i].draws[kind];
        self.seats[i].draws[kind] += 1;
        let delay = dist.sample(self.s.setup.run.seed, self.s.latency[i][kind], index);
        let at = event_at.max(self.now) + delay;
        let perception = (!anticipated).then(|| {
            let id = self.fresh_id();
            let st = &mut self.seats[i];
            st.unperceived.push(PendingPerception {
                reason,
                event_at,
                perceived_at: at,
            });
            st.perception_ids.push(id);
            id
        });
        self.queue.push(
            at,
            Event::Deliver(Wake {
                seat,
                reason,
                anticipated,
                event_at,
                gate,
                perception,
            }),
        );
    }

    /// Ask again at this timestamp, with no latency (free actions).
    pub(crate) fn deliver_now(&mut self, seat: Seat, reason: WakeReason) {
        self.queue.push(
            self.now,
            Event::Deliver(Wake {
                seat,
                reason,
                anticipated: true,
                event_at: self.now,
                gate: None,
                perception: None,
            }),
        );
    }

    pub(crate) fn perceive(&mut self, seat: Seat, perception: Option<u32>) {
        let Some(id) = perception else { return };
        let st = self.seat_mut(seat);
        if let Some(i) = st.perception_ids.iter().position(|&p| p == id) {
            st.perception_ids.remove(i);
            st.unperceived.remove(i);
        }
    }

    pub(crate) fn roll(&mut self, actor: ActorId, stream: StreamKey) -> f64 {
        let Some(a) = self.actors.get(usize::from(actor.0)) else {
            return 0.0;
        };
        let name = &a.name;
        let entry = self.streams.entry((actor, stream)).or_insert_with(|| {
            let key = stream.0.to_string();
            (rng::domain(Purpose::Mechanics, &[&**name, key.as_str()]), 0)
        });
        let u = rng::unit(self.s.setup.run.seed, entry.0, entry.1);
        entry.1 += 1;
        u
    }

    /// What pressing `ability` casts, after the seat's aura overrides.
    pub(crate) fn resolved(&self, seat: Seat, ability: SpellId) -> SpellId {
        let data = &self.s.setup.data;
        let Some(a) = self.actor_ref(self.seat_actor(seat)) else {
            return ability;
        };
        a.auras
            .iter()
            .filter_map(|inst| data.auras.get(&inst.aura))
            .flat_map(|def| &def.overrides)
            .find(|(from, _)| *from == ability)
            .map_or(ability, |&(_, to)| to)
    }

    pub(crate) fn predicate(
        &self,
        p: Predicate,
        caster: ActorId,
        target: Option<ActorId>,
        spell: SpellId,
    ) -> bool {
        match p {
            Predicate::TargetHasAura { aura, from_self } => {
                target.and_then(|t| self.actor_ref(t)).is_some_and(|t| {
                    t.auras
                        .iter()
                        .any(|i| i.aura == aura && (!from_self || i.source == caster))
                })
            }
            Predicate::CasterHasAura(aura) => self
                .actor_ref(caster)
                .is_some_and(|a| a.auras.iter().any(|i| i.aura == aura)),
            Predicate::TargetHpBelow(frac) => target
                .and_then(|t| self.actor_ref(t))
                .is_some_and(|t| t.max_health > 0 && t.health_frac() < frac),
            Predicate::DiffersFromLastCast => self
                .player_seat(caster)
                .and_then(|s| self.seat_ref(s))
                .and_then(|s| s.last_cast)
                .is_none_or(|last| last.spell != spell),
        }
    }

    /// Sum of the caster's modifiers of one kind that apply to `spell`, in
    /// the modifier's units.
    pub(crate) fn mod_sum(
        &self,
        caster: ActorId,
        spell: SpellId,
        kind: ModKind,
        target: Option<ActorId>,
    ) -> f64 {
        let data = &self.s.setup.data;
        let Some(a) = self.actor_ref(caster) else {
            return 0.0;
        };
        let sdef = data.spells.get(&spell);
        let mut total = 0.0;
        for inst in &a.auras {
            let Some(def) = data.auras.get(&inst.aura) else {
                continue;
            };
            for m in &def.modifiers {
                if m.kind != kind || !scope_matches(m.scope, spell, sdef) {
                    continue;
                }
                if m.condition
                    .is_some_and(|p| !self.predicate(p, caster, target, spell))
                {
                    continue;
                }
                let stacks = if m.per_stack {
                    f64::from(inst.stacks)
                } else {
                    1.0
                };
                total += m.value * stacks;
            }
        }
        total
    }

    pub(crate) fn cast_time_of(
        &self,
        caster: ActorId,
        spell: SpellId,
        target: Option<ActorId>,
    ) -> SimDuration {
        let Some(def) = self.s.setup.data.spells.get(&spell) else {
            return SimDuration::ZERO;
        };
        let (time, hasted) = match def.cast {
            CastKind::Instant | CastKind::Empower { .. } => return SimDuration::ZERO,
            CastKind::Cast { time, hasted } => (time, hasted),
            CastKind::Channel {
                duration, hasted, ..
            } => (duration, hasted),
        };
        let haste = self.actor_ref(caster).map_or(1.0, |a| a.haste);
        let mut ms = f64::from(time.millis());
        if hasted {
            ms /= haste;
        }
        let pct = self.mod_sum(caster, spell, ModKind::CastTimePct, target);
        millis_round(ms * (1.0 + pct / 100.0).max(0.0))
    }

    /// `PersistentPct` modifiers scoped to `Spell(id)` match the aura with
    /// the same numeric id, since that is how DoTs share ids with their
    /// spells.
    pub(crate) fn pmultiplier_of(&self, source: ActorId, aura: AuraId) -> f64 {
        let data = &self.s.setup.data;
        let Some(a) = self.actor_ref(source) else {
            return 1.0;
        };
        let mut mult = 1.0;
        for inst in &a.auras {
            let Some(def) = data.auras.get(&inst.aura) else {
                continue;
            };
            for m in &def.modifiers {
                let applies = match m.scope {
                    ModScope::All => true,
                    ModScope::Spell(s) => s.0 == aura.0,
                    ModScope::School(_) => false,
                };
                if m.kind == ModKind::PersistentPct && applies {
                    let stacks = if m.per_stack {
                        f64::from(inst.stacks)
                    } else {
                        1.0
                    };
                    mult *= 1.0 + m.value * stacks / 100.0;
                }
            }
        }
        mult
    }

    pub(crate) fn add_resource(&mut self, actor: ActorId, kind: ResourceKind, delta: f64) {
        let now = self.now;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let haste = a.haste;
        if let Some(r) = a.resources.iter_mut().find(|r| r.def.kind == kind) {
            r.settle(now, haste);
            r.value = (r.value + delta).clamp(0.0, r.def.max);
        }
    }

    pub(crate) fn set_regen_mult(&mut self, actor: ActorId, kind: ResourceKind, mult: f64) {
        let now = self.now;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let haste = a.haste;
        if let Some(r) = a.resources.iter_mut().find(|r| r.def.kind == kind) {
            r.settle(now, haste);
            r.regen_mult = mult;
        }
    }

    pub(crate) fn set_haste(&mut self, actor: ActorId, mult: f64) {
        let now = self.now;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let old = a.haste;
        let old_speed = a.swing_speed();
        for r in &mut a.resources {
            r.settle(now, old);
        }
        for cd in a.cooldowns.values_mut() {
            cd.settle(now, old);
        }
        a.haste = mult.max(f64::MIN_POSITIVE);
        let spells: Vec<SpellId> = a.cooldowns.keys().copied().collect();
        for spell in spells {
            self.schedule_cooldown(actor, spell);
        }
        self.rescale_swings(actor, old_speed);
        self.schedule_pet_act(actor, now);
    }

    pub(crate) fn set_attack_speed(&mut self, actor: ActorId, mult: f64) {
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let old_speed = a.swing_speed();
        a.attack_speed = mult.max(f64::MIN_POSITIVE);
        self.rescale_swings(actor, old_speed);
    }

    /// Returns the amount that landed, overkill included.
    pub(crate) fn damage(&mut self, d: crate::mechanics::DamageEvent) -> u64 {
        let Some(t) = self.actor_mut(d.target) else {
            return 0;
        };
        if !t.alive || d.amount == 0 {
            return 0;
        }
        let before = t.health;
        t.health = t.health.saturating_sub(d.amount);
        let died = t.health == 0;
        if let Some(seat) = self.owner_seat(d.source) {
            self.seat_mut(seat).outcome.damage_done += d.amount.min(before);
        }
        self.record(TraceEvent::Damage(d));
        if died {
            self.kill(d.target, Some(d.source));
        }
        self.queue_triggers();
        d.amount
    }

    pub(crate) fn heal(&mut self, h: crate::mechanics::HealEvent) {
        let Some(t) = self.actor_mut(h.target) else {
            return;
        };
        if !t.alive {
            return;
        }
        t.health = t.health.saturating_add(h.amount).min(t.max_health);
        self.record(TraceEvent::Heal(h));
        self.queue_triggers();
    }

    pub(crate) fn cancel_cast(&mut self, actor: ActorId, reason: crate::trace::CastEndReason) {
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let Some(c) = a.casting.take() else { return };
        self.record(TraceEvent::CastEnd {
            actor,
            spell: c.ev.spell,
            reason,
        });
    }

    pub(crate) fn kill(&mut self, actor: ActorId, killer: Option<ActorId>) {
        use crate::trace::CastEndReason;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        a.alive = false;
        self.cancel_cast(actor, CastEndReason::Interrupted);
        self.record(TraceEvent::Death { actor });
        self.followups
            .push_back(Followup::Died(DeathEvent { actor, killer }));
        while let Some(last) = self
            .actor_ref(actor)
            .and_then(|a| a.auras.len().checked_sub(1))
        {
            self.remove_instance(actor, last, crate::mechanics::AuraRemoval::HolderDied);
        }
        if let Some(seat) = self.player_seat(actor) {
            let st = self.seat_mut(seat);
            st.phase = SeatPhase::Dead;
            st.gen += 1;
            st.outcome.deaths += 1;
            if let Some(pull) = self.pulls.last_mut() {
                pull.deaths += 1;
            }
            return;
        }
        if self.is_pet(actor) {
            self.forget_pet(actor);
            return;
        }
        let next = self.live_targets().first().copied();
        let casting_at_it = |w: &World, id: ActorId| {
            w.actor_ref(id)
                .and_then(|a| a.casting)
                .is_some_and(|c| c.ev.target == Some(actor))
        };
        for i in 0..self.seats.len() {
            let seat = Seat(i as u8);
            let me = self.seat_actor(seat);
            if casting_at_it(self, me) {
                self.cancel_cast(me, CastEndReason::Interrupted);
                let st = self.seat_mut(seat);
                if st.phase == SeatPhase::Committed {
                    st.phase = SeatPhase::Locked;
                }
            }
            if let Some(a) = self.actor_mut(me) {
                if a.target == Some(actor) {
                    a.target = next;
                }
            }
            let now = self.now;
            self.notify(seat, WakeReason::EnemyDied(actor), false, None, now);
        }
        for pet in self.all_pets() {
            if casting_at_it(self, pet) {
                self.cancel_cast(pet, CastEndReason::Interrupted);
            }
            self.wake_pet(pet);
        }
    }

    pub(crate) fn engage(&mut self, enemy: ActorId) {
        let Some(a) = self.actor_mut(enemy) else {
            return;
        };
        if a.engaged || !a.alive {
            return;
        }
        a.engaged = true;
        self.record(TraceEvent::Engage { actor: enemy });
        let auras = self.s.setup.externals.enemy_auras.clone();
        for aura in auras {
            self.apply_aura(crate::mechanics::AuraApplication {
                aura: crate::state::AuraRef {
                    holder: enemy,
                    aura,
                    source: enemy,
                },
                stacks: 1,
                duration: None,
            });
        }
        for i in 0..self.seats.len() {
            let seat = Seat(i as u8);
            let me = self.seat_actor(seat);
            let needs_target = self
                .actor_ref(me)
                .is_some_and(|a| a.target.is_none_or(|t| !self.is_live_target(t)));
            if needs_target {
                if let Some(a) = self.actor_mut(me) {
                    a.target = Some(enemy);
                }
            }
            let now = self.now;
            self.notify(seat, WakeReason::EnemyEngaged(enemy), false, None, now);
        }
        for pet in self.all_pets() {
            self.wake_pet(pet);
        }
        self.queue_triggers();
    }

    pub(crate) fn launch(&mut self, ev: CastEvent, travel: SimDuration) {
        let Some(target) = ev.target else { return };
        let id = self.fresh_id();
        let lands = self.now + travel;
        self.projectiles.push(Projectile {
            spell: ev.spell,
            source: ev.actor,
            target,
            launched: self.now,
            lands,
        });
        self.flights.push((id, ev));
        self.record(TraceEvent::ProjectileLaunched {
            actor: ev.actor,
            spell: ev.spell,
            target,
            lands,
        });
        self.queue.push(lands, Event::ProjectileLand { id });
    }

    pub(crate) fn schedule_timer(
        &mut self,
        delay: SimDuration,
        timer: crate::mechanics::TimerEvent,
    ) {
        let id = self.fresh_id();
        let fires_at = self.now + delay;
        let at = self.timers.partition_point(|t| t.fires_at <= fires_at);
        self.timers.insert(at, PendingTimer { timer, fires_at });
        self.timer_ids.insert(at, id);
        self.queue.push(fires_at, Event::Timer { id });
    }

    pub(crate) fn seat_outcomes(&self) -> Vec<SeatOutcome> {
        self.seats.iter().map(|s| s.outcome.clone()).collect()
    }
}

impl StateView for World {
    fn now(&self) -> SimTime {
        self.now
    }

    fn segment(&self) -> SegmentView {
        match self.seg {
            Seg::Travel {
                combat,
                prepull_from,
                combat_starts,
                ..
            } => SegmentView::Travel {
                next_combat: combat,
                prepull_from,
                combat_starts,
            },
            Seg::Combat {
                combat,
                started,
                deadline,
                ..
            } => SegmentView::Combat(CombatView {
                index: combat,
                started,
                deadline,
            }),
            Seg::Finished => SegmentView::Finished,
        }
    }

    fn seats(&self) -> &[ActorId] {
        &self.seat_actors
    }

    fn enemies(&self) -> &[ActorId] {
        &self.enemies
    }

    fn actor(&self, id: ActorId) -> Option<ActorView> {
        self.actor_ref(id).map(|a| ActorView {
            kind: a.kind,
            health: a.health,
            max_health: a.max_health,
            alive: a.alive,
            engaged: a.engaged,
            casting: a.casting.map(|c| CastView {
                what: CastWhat::Spell(c.ev.spell),
                started: c.ev.started,
                ends: c.ends,
                interruptible: false,
                next_tick: None,
                empower_stage: None,
                next_stage_at: None,
            }),
            moving_until: None,
            expires: a.pet.and_then(|p| p.expires),
        })
    }

    fn resource(&self, id: ActorId, kind: ResourceKind) -> Option<ResourceView> {
        let a = self.actor_ref(id)?;
        let r = a.resources.iter().find(|r| r.def.kind == kind)?;
        Some(ResourceView {
            value: r.value_at(self.now, a.haste),
            max: r.def.max,
            regen_per_sec: r.rate(a.haste),
        })
    }

    fn cooldown(&self, id: ActorId, spell: SpellId) -> Option<CooldownView> {
        let a = self.actor_ref(id)?;
        let cd = match a.cooldowns.get(&spell) {
            Some(cd) => {
                let mut cd = *cd;
                cd.settle(self.now, a.haste);
                cd
            }
            None => self.fresh_cooldown(id, spell)?,
        };
        let rate = cd.rate(a.haste);
        Some(CooldownView {
            charges: cd.charges,
            max_charges: cd.max,
            progress: if cd.charges < cd.max && cd.base > 0.0 {
                (cd.progress / cd.base).clamp(0.0, 1.0 - f64::EPSILON)
            } else {
                0.0
            },
            next_charge_at: cd.ready_at(a.haste),
            recharge: millis_ceil(cd.base / rate),
        })
    }

    fn gcd_end(&self, seat: Seat) -> Option<SimTime> {
        self.seat_ref(seat)?.gcd_end.filter(|&t| t > self.now)
    }

    fn gcd_length(&self, seat: Seat) -> SimDuration {
        let haste = self
            .actor_ref(self.seat_actor(seat))
            .map_or(1.0, |a| a.haste);
        self.s
            .gcd
            .get(usize::from(seat.0))
            .copied()
            .flatten()
            .map_or(SimDuration::ZERO, |g| gcd_length(&g, haste))
    }

    fn resolved_spell(&self, seat: Seat, ability: SpellId) -> SpellId {
        self.resolved(seat, ability)
    }

    fn cast_time(&self, seat: Seat, ability: SpellId) -> SimDuration {
        let actor = self.seat_actor(seat);
        let target = self.actor_ref(actor).and_then(|a| a.target);
        self.cast_time_of(actor, self.resolved(seat, ability), target)
    }

    fn phase(&self, seat: Seat) -> SeatPhase {
        self.seat_ref(seat).map_or(SeatPhase::Dead, |s| s.phase)
    }

    fn auras(&self, holder: ActorId) -> &[AuraInstance] {
        self.actor_ref(holder).map_or(&[], |a| &a.auras)
    }

    fn procs(&self, holder: ActorId) -> &[ProcView] {
        self.actor_ref(holder).map_or(&[], |a| &a.procs)
    }

    fn pmultiplier(&self, source: ActorId, aura: AuraId) -> f64 {
        self.pmultiplier_of(source, aura)
    }

    fn pets(&self, owner: Seat) -> &[ActorId] {
        self.pets
            .get(usize::from(owner.0))
            .map_or(&[], Vec::as_slice)
    }

    fn target(&self, id: ActorId) -> Option<ActorId> {
        let a = self.actor_ref(id)?;
        match a.kind {
            ActorKind::Pet { owner, .. } => self.actor_ref(self.seat_actor(owner))?.target,
            ActorKind::Player(_) | ActorKind::Enemy { .. } => a.target,
        }
    }

    fn swing(&self, id: ActorId, hand: WeaponHand) -> Option<SwingView> {
        let a = self.actor_ref(id)?;
        let s = a.swings[hand_index(hand)]?;
        Some(SwingView {
            next_at: s.next_at?,
            interval: s.interval(a.swing_speed()),
        })
    }

    fn projectiles(&self) -> &[Projectile] {
        &self.projectiles
    }

    fn last_cast(&self, seat: Seat) -> Option<LastCast> {
        self.seat_ref(seat)?.last_cast
    }

    fn rule(&self, _enemy: ActorId, _rule: RuleIndex) -> RuleView {
        RuleView {
            fired: 0,
            last_fired: None,
            active: false,
        }
    }

    fn enemy_phase(&self, enemy: ActorId) -> Option<&PhaseName> {
        self.actor_ref(enemy)?.phase.as_ref()
    }

    fn timers(&self) -> &[PendingTimer] {
        &self.timers
    }

    fn unperceived(&self, seat: Seat) -> &[PendingPerception] {
        self.seat_ref(seat).map_or(&[], |s| &s.unperceived)
    }
}
