//! Read-only view of rollout state.
//!
//! This is privileged truth. Only observers (in `portunus-env`) and
//! mechanics read it; policies see what an observer chooses to show them.
//!
//! The state is complete, so decisions are Markov: nothing about the past
//! matters except what is still present here. Facts a player would "remember"
//! are kept as current facts instead — a cooldown's remaining time rather
//! than when it was used, when an enemy rule last fired rather than a log of
//! its casts. Timings the engine derives (hasted GCD, cast times,
//! recharge progress) are exposed directly so observers never re-derive
//! game formulas.

use portunus_core::{ActorId, AuraId, EnemyKey, PetId, Seat, SimDuration, SimTime, SpellId};
use portunus_gamedata::effect::Predicate;
pub use portunus_gamedata::effect::RECENT_CASTS;
use portunus_gamedata::enemy::{EnemyDef, PhaseName, RuleIndex};
use portunus_gamedata::item::WeaponHand;
use portunus_gamedata::stats::ResourceKind;
use portunus_scenario::resolved::SpawnIndex;
use serde::{Deserialize, Serialize};

use crate::choice::MoveGoal;
use crate::mechanics::TimerEvent;
use crate::step::WakeReason;

/// What an enemy actor is and where it came from.
#[derive(Debug, Clone, Copy)]
pub struct EnemyInfo<'a> {
    pub key: &'a EnemyKey,
    /// `None` if the enemy data lacks `key`.
    pub def: Option<&'a EnemyDef>,
    pub forces: u32,
    /// The enemy whose rule spawned it; `None` for the pull's own.
    pub spawner: Option<ActorId>,
    /// It left with its spawner rather than dying.
    pub despawned: bool,
}

pub trait StateView {
    fn now(&self) -> SimTime;
    fn segment(&self) -> SegmentView;
    /// Seat index to actor.
    fn seats(&self) -> &[ActorId];
    /// Every enemy spawned so far in the current combat, engaged or not.
    fn enemies(&self) -> &[ActorId];
    /// What an enemy of any combat so far is.
    fn enemy_info(&self, enemy: ActorId) -> Option<EnemyInfo<'_>>;
    /// The forces of a combat's adds slain so far (not despawned).
    fn add_forces(&self, combat: u16) -> u32;
    fn actor(&self, id: ActorId) -> Option<ActorView>;
    fn resource(&self, id: ActorId, kind: ResourceKind) -> Option<ResourceView>;
    /// Any actor's cooldown on a spell: a seat's abilities (teammates'
    /// included) or a pet's spells.
    fn cooldown(&self, id: ActorId, spell: SpellId) -> Option<CooldownView>;
    fn gcd_end(&self, seat: Seat) -> Option<SimTime>;
    /// How long a GCD started now would last.
    fn gcd_length(&self, seat: Seat) -> SimDuration;
    /// What pressing `ability` would cast right now (after aura overrides).
    fn resolved_spell(&self, seat: Seat, ability: SpellId) -> SpellId;
    /// How long that cast would take if started now (0 for instants).
    fn cast_time(&self, seat: Seat, ability: SpellId) -> SimDuration;
    fn phase(&self, seat: Seat) -> SeatPhase;
    fn auras(&self, holder: ActorId) -> &[AuraInstance];
    /// The auras `actor` held as it last died, before death removed them;
    /// empty if it never died.
    fn auras_at_death(&self, actor: ActorId) -> &[AuraInstance];
    /// An enemy's place in its pack: yards along the pack, and yards from
    /// the party (moved only by the enemy repositioning). Enemies are as
    /// far apart as these points are.
    fn pack_position(&self, enemy: ActorId) -> Option<(f64, f64)>;
    /// Proc bookkeeping for the listeners on this holder's auras.
    fn procs(&self, holder: ActorId) -> &[ProcView];
    /// The persistent multiplier (`ModKind::PersistentPct`) this aura would
    /// snapshot if `source` applied it now; compare with the instance's
    /// `pmultiplier` to decide whether a refresh is worth it.
    fn pmultiplier(&self, source: ActorId, aura: AuraId) -> f64;
    /// This seat's live pets, guardians, and totems, oldest first.
    fn pets(&self, owner: Seat) -> &[ActorId];
    /// Primary target of a seat or pet (pets follow their owner's).
    fn target(&self, id: ActorId) -> Option<ActorId>;
    /// Auto-attack timer for an equipped weapon.
    fn swing(&self, id: ActorId, hand: WeaponHand) -> Option<SwingView>;
    /// Spells launched and not yet landed.
    fn projectiles(&self) -> &[Projectile];
    /// The seat's last [`RECENT_CASTS`] completed (or released) casts,
    /// newest first. A cast joins once its `cast_completed` effects have
    /// run, so those effects see the casts before it.
    fn recent_casts(&self, seat: Seat) -> [Option<LastCast>; RECENT_CASTS];
    /// The seat's most recent completed (or released) cast.
    fn last_cast(&self, seat: Seat) -> Option<LastCast> {
        self.recent_casts(seat)[0]
    }
    fn rule(&self, enemy: ActorId, rule: RuleIndex) -> RuleView;
    /// An enemy's current phase; `None` for enemies without phases.
    fn enemy_phase(&self, enemy: ActorId) -> Option<&PhaseName>;
    /// Mechanics timers not yet fired, soonest first.
    fn timers(&self) -> &[PendingTimer];
    /// Unanticipated events that have happened but that this seat hasn't
    /// perceived yet, because their reaction delays are still running.
    /// Realistic observers hide these (and any legality they enable).
    fn unperceived(&self, seat: Seat) -> &[PendingPerception];
    /// The seat's current movement, if it is moving.
    fn movement(&self, seat: Seat) -> Option<MovementView>;
    /// Movement the seat owes, as of now, soonest deadline first.
    fn demands(&self, seat: Seat) -> Vec<DemandView>;
    /// The seat's run speed in yards per second.
    fn run_speed(&self, seat: Seat) -> f64;
    /// Yards between the seat and an enemy, as of now; `None` if `enemy`
    /// isn't one.
    fn distance(&self, seat: Seat, enemy: ActorId) -> Option<f64>;
    /// Yards the seat has moved, by any means, since the run began.
    fn yards_travelled(&self, seat: Seat) -> f64;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementView {
    pub started: SimTime,
    /// When it stops at the current speed: the goal reached, or the forced
    /// movement over.
    pub ends: SimTime,
    /// Forced movement can't be stopped.
    pub forced: bool,
    /// `None` while forced.
    pub goal: Option<MoveGoal>,
}

/// Movement owed by a deadline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DemandView {
    pub source: ActorId,
    pub rule: RuleIndex,
    /// Yards still to cover.
    pub yards: f64,
    pub placed: SimTime,
    pub deadline: SimTime,
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
pub struct PendingTimer {
    pub timer: TimerEvent,
    pub fires_at: SimTime,
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
    /// Whole points, as in the game; hits are rounded before they land.
    pub health: u64,
    pub max_health: u64,
    pub alive: bool,
    pub engaged: bool,
    pub casting: Option<CastView>,
    pub moving_until: Option<SimTime>,
    /// When a guardian or totem despawns; `None` for everything else.
    pub expires: Option<SimTime>,
}

impl ActorView {
    /// Health left as a fraction of maximum, in `[0, 1]`; 0 with no
    /// maximum.
    pub fn health_frac(&self) -> f64 {
        if self.max_health == 0 {
            0.0
        } else {
            self.health as f64 / self.max_health as f64
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    Player(Seat),
    Enemy {
        combat: u16,
        spawn: SpawnIndex,
    },
    /// A pet, guardian, or totem. None get decision points: they act on
    /// their autocast and on what their owner's effects command.
    Pet {
        owner: Seat,
        pet: PetId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastView {
    pub what: CastWhat,
    pub started: SimTime,
    /// When the cast or channel completes; for empowers, when the hold runs
    /// out.
    pub ends: SimTime,
    pub interruptible: bool,
    /// Channels: when the next tick lands.
    pub next_tick: Option<SimTime>,
    /// Channels: ticks landed so far, of how many.
    pub ticks: Option<ChannelProgress>,
    /// Empowers: the stage reached so far (0 before the first).
    pub empower_stage: Option<u8>,
    /// Empowers: when the next stage is reached, if any remain.
    pub next_stage_at: Option<SimTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelProgress {
    pub done: u8,
    pub total: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastWhat {
    Spell(SpellId),
    EnemyRule(RuleIndex),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceView {
    pub value: f64,
    /// Current maximum (talents and auras can change it).
    pub max: f64,
    /// 0 for a resource that refills a unit at a time.
    pub regen_per_sec: f64,
    /// For one that refills a unit at a time (runes): when each missing
    /// unit is back if nothing changes, soonest first.
    pub next_ready: Vec<SimTime>,
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

/// One aura instance: auras are held per `(holder, aura, source)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AuraRef {
    pub holder: ActorId,
    pub aura: AuraId,
    pub source: ActorId,
}

/// One listener on an aura instance, by its index in `AuraDef::listeners`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ListenerRef {
    pub aura: AuraRef,
    pub index: u8,
}

/// Everything a proc's chance depends on besides its definition and the
/// holder's haste. Kept by the kernel for as long as the aura instance
/// lasts. A player who watches their procs knows all of it; the outcome of
/// the next roll is never part of the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcView {
    pub listener: ListenerRef,
    /// When the listener last rolled (RPPM's chance grows with this gap).
    pub last_attempt: Option<SimTime>,
    /// When it last succeeded (RPPM's bad-luck protection grows with this
    /// gap).
    pub last_proc: Option<SimTime>,
    /// When the internal cooldown ends, if one is running.
    pub icd_ready_at: Option<SimTime>,
    /// What is left in a deck-of-cards listener's current deck. Unlike the
    /// rest, the deck outlives the instance: it carries over to the holder's
    /// next instance of the aura for the rest of the run.
    pub deck: Option<DeckView>,
}

/// The undrawn part of a shuffled deck: the next draw procs with chance
/// `successes / cards`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckView {
    pub cards: u16,
    pub successes: u16,
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
    /// Where a ground aura (`AuraDef::ground`) was placed.
    pub ground: Option<GroundView>,
}

/// A ground aura's placement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GroundView {
    /// A pack position (see [`StateView::pack_position`]); `None` if there
    /// was no enemy to centre it on, in which case it reaches every enemy.
    pub centre: Option<(f64, f64)>,
    pub radius: f64,
    /// The holder's [`StateView::yards_travelled`] when it was placed.
    pub placed_at_yards: f64,
}

/// Yards an enemy's body adds to a ground effect's radius: SimC's default
/// `combat_reach`, which `action_t::check_distance_targeting` adds for
/// ground AoE.
pub const ENEMY_COMBAT_REACH: f64 = 1.0;

impl GroundView {
    /// Whether an enemy at this pack position is inside.
    pub fn reaches(&self, at: (f64, f64)) -> bool {
        self.centre
            .is_none_or(|(x, y)| (at.0 - x).hypot(at.1 - y) <= self.radius + ENEMY_COMBAT_REACH)
    }

    /// Yards the holder can still move before it is outside, given its
    /// [`StateView::yards_travelled`] now; negative once it has left.
    pub fn yards_left(&self, travelled: f64) -> f64 {
        self.radius - (travelled - self.placed_at_yards)
    }
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

/// Whether `p` holds for this caster, target, and spell: the one evaluator
/// for conditional modifiers, listener conditions, and `Effect::If`.
pub fn predicate(
    view: &dyn StateView,
    p: Predicate,
    caster: ActorId,
    target: Option<ActorId>,
    spell: Option<SpellId>,
) -> bool {
    let has = |holder: ActorId,
               aura: AuraId,
               from: Option<ActorId>,
               ok: &dyn Fn(&AuraInstance) -> bool| {
        view.auras(holder)
            .iter()
            .any(|i| i.aura == aura && from.is_none_or(|f| i.source == f) && ok(i))
    };
    let target_view = || target.and_then(|t| view.actor(t));
    let caster_view = || view.actor(caster);
    let seat = || match caster_view()?.kind {
        ActorKind::Player(seat) => Some(seat),
        ActorKind::Enemy { .. } | ActorKind::Pet { .. } => None,
    };
    let any = |_: &AuraInstance| true;
    match p {
        Predicate::TargetHasAura { aura, from_self } => {
            target.is_some_and(|t| has(t, aura, from_self.then_some(caster), &any))
        }
        Predicate::CasterHasAura(aura) => has(caster, aura, None, &any),
        Predicate::CasterLacksAura(aura) => !has(caster, aura, None, &any),
        Predicate::OwnerHasAura(aura) => match caster_view().map(|a| a.kind) {
            Some(ActorKind::Pet { owner, .. }) => view
                .seats()
                .get(usize::from(owner.0))
                .is_some_and(|&o| has(o, aura, None, &any)),
            _ => false,
        },
        Predicate::TargetHpBelow(frac) => {
            target_view().is_some_and(|t| t.max_health > 0 && t.health_frac() < frac)
        }
        Predicate::TargetHpAbove(frac) => {
            target_view().is_some_and(|t| t.max_health > 0 && t.health_frac() > frac)
        }
        Predicate::CasterHpBelow(frac) => {
            caster_view().is_some_and(|c| c.max_health > 0 && c.health_frac() < frac)
        }
        Predicate::CasterHpAbove(frac) => {
            caster_view().is_some_and(|c| c.max_health > 0 && c.health_frac() > frac)
        }
        Predicate::TargetStacksAtLeast {
            aura,
            stacks,
            from_self,
        } => target.is_some_and(|t| {
            has(t, aura, from_self.then_some(caster), &|i| {
                i.stacks >= stacks
            })
        }),
        Predicate::CasterStacksAtLeast { aura, stacks } => {
            has(caster, aura, None, &|i| i.stacks >= stacks)
        }
        Predicate::AuraValueAtLeast { aura, value } => {
            has(caster, aura, None, &|i| i.value >= value)
        }
        Predicate::TargetHpBelowCasterMaxHp => match (target_view(), caster_view()) {
            (Some(t), Some(c)) => t.health > 0 && t.health <= c.max_health,
            _ => false,
        },
        Predicate::DiffersFromLastCast => seat()
            .and_then(|s| view.last_cast(s))
            .is_none_or(|last| Some(last.spell) != spell),
        Predicate::RecentCasts { spell, count } => seat().is_some_and(|s| {
            let n = usize::from(count);
            let recent = view.recent_casts(s);
            n <= RECENT_CASTS
                && recent[..n]
                    .iter()
                    .all(|c| c.is_some_and(|c| c.spell == spell))
        }),
        Predicate::InOwnGround(aura) => has(caster, aura, Some(caster), &|i| {
            i.ground.is_some_and(|g| {
                seat().is_none_or(|s| g.yards_left(view.yards_travelled(s)) >= 0.0)
            })
        }),
    }
}

/// One enemy rule's current status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleView {
    pub fired: u32,
    pub last_fired: Option<SimTime>,
    /// Active in the enemy's current phase.
    pub active: bool,
}
