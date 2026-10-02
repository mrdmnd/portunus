//! Read-only view of rollout state.
//!
//! This is privileged truth. Only observers (in `portunus-env`) and
//! mechanics read it; policies see what an observer chooses to show them.
//!
//! The state is complete, so decisions are Markov: nothing about the past
//! matters except what is still present here. Facts a player would "remember"
//! are kept as current facts instead — a cooldown's remaining time rather
//! than when it was used, time since an enemy rule last fired rather than a
//! log of its casts. Timings the engine derives (hasted GCD, cast times,
//! recharge progress) are exposed directly so observers never re-derive
//! game formulas.

use portunus_core::{AbilitySlot, ActorId, AuraId, PetId, Seat, SimDuration, SimTime, SpellId};
use portunus_gamedata::enemy::RuleIndex;
use portunus_gamedata::item::WeaponHand;
use portunus_gamedata::stats::ResourceKind;
use portunus_scenario::resolved::SpawnIndex;

use crate::step::WakeReason;

pub trait StateView {
    fn now(&self) -> SimTime;
    fn segment(&self) -> SegmentView;
    /// Seat index to actor.
    fn seats(&self) -> &[ActorId];
    /// Every enemy spawned so far in the current combat, engaged or not.
    fn enemies(&self) -> &[ActorId];
    fn actor(&self, id: ActorId) -> Option<ActorView>;
    fn resource(&self, id: ActorId, kind: ResourceKind) -> Option<ResourceView>;
    /// Any seat's cooldowns, including teammates'.
    fn cooldown(&self, seat: Seat, slot: AbilitySlot) -> Option<CooldownView>;
    /// A pet's cooldown on one of its spells (autocast or commanded).
    fn pet_cooldown(&self, pet: ActorId, spell: SpellId) -> Option<CooldownView>;
    fn gcd_end(&self, seat: Seat) -> Option<SimTime>;
    /// How long a GCD started now would last.
    fn gcd_length(&self, seat: Seat) -> SimDuration;
    /// What a slot would cast right now (after aura overrides).
    fn resolved_spell(&self, seat: Seat, slot: AbilitySlot) -> SpellId;
    /// How long that cast would take if started now (0 for instants).
    fn cast_time(&self, seat: Seat, slot: AbilitySlot) -> SimDuration;
    fn phase(&self, seat: Seat) -> SeatPhase;
    fn auras(&self, holder: ActorId) -> &[AuraInstance];
    /// The persistent multiplier (`ModKind::PersistentPct`) this aura would
    /// snapshot if `source` applied it now; compare with the instance's
    /// `pmultiplier` to decide whether a refresh is worth it.
    fn pmultiplier(&self, source: ActorId, aura: AuraId) -> f64;
    /// This seat's live pets and guardians, oldest first.
    fn pets(&self, owner: Seat) -> &[ActorId];
    /// Primary target of a seat or pet (pets follow their owner's).
    fn target(&self, id: ActorId) -> Option<ActorId>;
    /// Auto-attack timer for an equipped weapon.
    fn swing(&self, id: ActorId, hand: WeaponHand) -> Option<SwingView>;
    /// Spells launched and not yet landed.
    fn projectiles(&self) -> &[Projectile];
    /// The seat's most recent completed (or released) cast.
    fn last_cast(&self, seat: Seat) -> Option<LastCast>;
    fn rule(&self, enemy: ActorId, rule: RuleIndex) -> RuleView;
    /// Unanticipated events that have happened but that this seat hasn't
    /// perceived yet, because their reaction delays are still running.
    /// Realistic observers hide these (and any legality they enable).
    fn unperceived(&self, seat: Seat) -> &[PendingPerception];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentView {
    /// Between pulls. Seats can act once `prepull_from` is reached.
    Travel {
        next_combat: u16,
        prepull_from: SimTime,
        combat_starts: SimTime,
    },
    Combat(CombatView),
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatView {
    /// Index among the run's combat segments.
    pub index: u16,
    pub started: SimTime,
    pub deadline: SimTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingPerception {
    pub reason: WakeReason,
    pub event_at: SimTime,
    pub perceived_at: SimTime,
}

/// Where a seat is in the decision cycle. See the crate docs for when each
/// transition produces a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatPhase {
    /// Mid cast or channel; only `usable_while_casting` spells and
    /// `StopCast` are available.
    Committed,
    /// On GCD with nothing usable off it; sleeps until the mask changes.
    Locked,
    /// Parked on a wait it named.
    Waiting,
    /// Has a pending or scheduled decision request.
    Deciding,
    /// Travelling, before the pre-pull window opens.
    Idle,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorView {
    pub kind: ActorKind,
    pub health: f64,
    pub max_health: f64,
    pub alive: bool,
    pub engaged: bool,
    pub casting: Option<CastView>,
    pub moving_until: Option<SimTime>,
    /// When a guardian despawns; `None` for everything else.
    pub expires: Option<SimTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    Player(Seat),
    Enemy {
        combat: u16,
        spawn: SpawnIndex,
    },
    /// Pets never get decision points; they act on their own definitions.
    Pet {
        owner: Seat,
        pet: PetId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastView {
    pub what: CastWhat,
    pub started: SimTime,
    /// For empowers, when the hold runs out.
    pub ends: SimTime,
    pub interruptible: bool,
    /// Channels: when the next tick lands.
    pub next_tick: Option<SimTime>,
    /// Empowers: the stage reached so far (0 before the first).
    pub empower_stage: Option<u8>,
    /// Empowers: when the next stage is reached, if any remain.
    pub next_stage_at: Option<SimTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastWhat {
    Spell(SpellId),
    EnemyRule(RuleIndex),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResourceView {
    pub value: f64,
    /// Current maximum (talents and auras can change it).
    pub max: f64,
    pub regen_per_sec: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CooldownView {
    pub charges: u8,
    pub max_charges: u8,
    /// Fraction of the next charge recovered, in `[0, 1)`; 0 when full.
    /// `charges + progress` is SimC's `charges_fractional`.
    pub progress: f64,
    /// When the next charge returns at the current rate, if recharging.
    pub next_charge_at: Option<SimTime>,
    /// Full recharge time at the current rate.
    pub recharge: SimDuration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuraInstance {
    pub aura: AuraId,
    pub source: ActorId,
    pub stacks: u8,
    /// `None` for permanent auras. For `RefreshRule::Ironfur`, the newest
    /// stack's expiry.
    pub expires: Option<SimTime>,
    /// For `RefreshRule::Ironfur`, when the oldest stack drops.
    pub next_stack_expires: Option<SimTime>,
    /// The aura's carried value (see `AuraValue`); 0 if it has none.
    pub value: f64,
    /// Persistent multiplier snapshotted at application; 1 if none.
    pub pmultiplier: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwingView {
    pub next_at: SimTime,
    /// Current hasted interval.
    pub interval: SimDuration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Projectile {
    pub spell: SpellId,
    pub source: ActorId,
    pub target: ActorId,
    pub launched: SimTime,
    pub lands: SimTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LastCast {
    pub spell: SpellId,
    pub at: SimTime,
}

/// One enemy rule's current status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleView {
    pub fired: u32,
    pub last_fired: Option<SimTime>,
    /// Active in the enemy's current phase.
    pub active: bool,
}
