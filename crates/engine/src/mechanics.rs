//! The plug-in point for game semantics.
//!
//! Rule: mechanics hold no mutable state of their own. Everything that
//! changes during a rollout lives in kernel primitives (auras, resources,
//! cooldowns, timers, proc bookkeeping), which keeps forks cheap and all
//! state observable.

use portunus_core::{ActorId, AuraId, PetId, Seat, SimDuration, SimTime, SpellId, StreamKey};
use portunus_gamedata::effect::{CooldownChange, Effect};
use portunus_gamedata::enemy::RuleIndex;
use portunus_gamedata::item::WeaponHand;
use portunus_gamedata::stats::{ResourceAmount, ResourceKind, SchoolMask};
use portunus_gamedata::GameData;
use serde::{Deserialize, Serialize};

use crate::mask::Readiness;
use crate::state::{AuraRef, ListenerRef, Projectile, StateView};
use crate::step::WakeReason;

pub trait Mechanics: Clone {
    /// Usability beyond the kernel's generic gates (cost, cooldown, GCD,
    /// casting, movement), e.g. "only during this proc".
    fn gate(&self, view: &dyn StateView, seat: Seat, ability: SpellId) -> Readiness;
    fn combat_started(&self, io: &mut dyn EngineIo, combat: u16);
    fn combat_ended(&self, io: &mut dyn EngineIo, combat: u16, cleared: bool);
    fn cast_started(&self, io: &mut dyn EngineIo, cast: &CastEvent);
    /// The cast succeeded, its costs paid and cooldown started: as a hard
    /// cast completes, as a channel starts (its effects belong to
    /// [`Mechanics::channel_tick`]), or as an empower is released past its
    /// first stage (`cast.empower` holds the stage).
    fn cast_completed(&self, io: &mut dyn EngineIo, cast: &CastEvent);
    /// Tick `tick` (from 1) of a channel; the last lands as it ends.
    fn channel_tick(&self, io: &mut dyn EngineIo, cast: &CastEvent, tick: u8);
    /// A spell with travel time reached its target, carrying the hits
    /// stashed for it at launch ([`EngineIo::stash_hit`]).
    fn projectile_landed(
        &self,
        io: &mut dyn EngineIo,
        cast: &CastEvent,
        flight: &Projectile,
        hits: &[RolledHit],
    );
    /// An auto-attack swing resolved.
    fn swing(&self, io: &mut dyn EngineIo, swing: &SwingEvent);
    fn periodic_tick(&self, io: &mut dyn EngineIo, tick: &TickEvent);
    /// An aura was applied or refreshed, or its stack count changed without
    /// it being removed. Covers every application, including run-start
    /// passives and externals.
    fn aura_changed(&self, io: &mut dyn EngineIo, ev: &AuraChange);
    fn aura_removed(&self, io: &mut dyn EngineIo, ev: &AuraEvent);
    fn actor_died(&self, io: &mut dyn EngineIo, ev: &DeathEvent);
    /// A temporary pet reached the end of its lifetime and is already gone;
    /// dismissals and replacements don't count.
    fn pet_expired(&self, io: &mut dyn EngineIo, ev: &PetEvent);
    /// A valued aura's value reached its threshold and dropped by it (see
    /// [`ValueLimits::threshold`]); called once per crossing.
    fn aura_threshold(&self, io: &mut dyn EngineIo, aura: AuraRef);
    /// An enemy rule hit a player: apply mitigation, then call
    /// [`EngineIo::apply_damage`].
    fn enemy_hit(&self, io: &mut dyn EngineIo, hit: &EnemyHit);
    /// Run effects cast by an enemy at a player: an enemy rule's `Effects`
    /// action, or a movement demand's failure.
    fn enemy_effects(
        &self,
        io: &mut dyn EngineIo,
        source: ActorId,
        target: ActorId,
        effects: &[Effect],
    );
    fn timer(&self, io: &mut dyn EngineIo, timer: &TimerEvent);
    /// An absorb soaked part of a hit, before any removal for running
    /// out.
    fn aura_absorbed(&self, _io: &mut dyn EngineIo, _ev: &AbsorbEvent) {}
    /// A seat started moving from standing still, or stopped.
    fn movement_changed(&self, _io: &mut dyn EngineIo, _seat: Seat, _moving: bool) {}
    /// The aura's `prevents_death` saved its holder from a lethal hit:
    /// health is already set and the lockout applied.
    fn death_prevented(&self, _io: &mut dyn EngineIo, _aura: AuraRef) {}
}

/// What mechanics may do to the world. The kernel handles scheduling,
/// expiry, ticking, and wake-ups that follow from each call.
pub trait EngineIo {
    fn view(&self) -> &dyn StateView;
    fn data(&self) -> &GameData;
    /// Absorbs on the target take their share first, smallest shield first
    /// (SimC's `account_absorb_buffs`); the rest comes off its health.
    fn apply_damage(&mut self, d: DamageEvent) -> Landed;
    fn apply_heal(&mut self, h: HealEvent);
    fn apply_aura(&mut self, a: AuraApplication);
    /// `source: None` removes the aura from every source.
    fn remove_aura(&mut self, holder: ActorId, aura: AuraId, source: Option<ActorId>);
    /// Remove up to `stacks` stacks; the aura goes when none remain.
    fn remove_stacks(&mut self, aura: AuraRef, stacks: u8);
    /// Push back an existing aura's expiry (no-op if absent).
    fn extend_aura(&mut self, aura: AuraRef, by: SimDuration);
    /// Change a valued aura's value, applying the aura if absent, then
    /// enforce `limits`, which mechanics evaluate from the aura's
    /// coefficients. An absorb whose value runs out is removed as
    /// [`AuraRemoval::Depleted`].
    fn add_aura_value(&mut self, aura: AuraRef, delta: f64, limits: ValueLimits);
    /// A listener rolled for a proc. The kernel updates its
    /// [`crate::state::ProcView`] and, on success, starts its internal
    /// cooldown.
    fn record_proc_attempt(&mut self, listener: ListenerRef, procced: bool);
    fn add_resource(&mut self, actor: ActorId, kind: ResourceKind, delta: f64);
    fn set_regen_mult(&mut self, actor: ActorId, kind: ResourceKind, mult: f64);
    /// Rescales GCDs, casts, hasted cooldowns, regen, and ticks from now on.
    fn set_haste(&mut self, actor: ActorId, mult: f64);
    /// Rescales swing timers from now on, on top of haste.
    fn set_attack_speed(&mut self, actor: ActorId, mult: f64);
    /// `actor` is a seat's actor or a pet.
    fn adjust_cooldown(&mut self, actor: ActorId, spell: SpellId, change: CooldownChange);
    /// Spawn pets; returns their actors. `duration` is ignored for
    /// permanent pets.
    fn summon(
        &mut self,
        owner: Seat,
        pet: PetId,
        count: u8,
        duration: Option<SimDuration>,
    ) -> Vec<ActorId>;
    /// Remove an owner's pets of a type, oldest first (`None`: all).
    fn dismiss(&mut self, owner: Seat, pet: PetId, count: Option<u8>);
    /// Extend an owner's guardians (`pet: None` extends every type).
    fn extend_pets(&mut self, owner: Seat, pet: Option<PetId>, by: SimDuration);
    /// Make an owner's pets cast `spell` at `target` outside their autocast
    /// priority (`pet: None` commands every type).
    fn command_pets(&mut self, owner: Seat, pet: Option<PetId>, spell: SpellId, target: ActorId);
    /// Cast `spell` for free: no gates, costs, cast time, or cooldown. It
    /// resolves through [`Mechanics::cast_completed`] as a new event `delay`
    /// from now, never re-entrantly, then through
    /// [`Mechanics::projectile_landed`] after its travel time, if it has one.
    ///
    /// `rolled` carries hits rolled now, as SimC snapshots an overload when
    /// its parent launches: they are delivered through
    /// [`Mechanics::projectile_landed`] (at once if the spell doesn't
    /// travel), and the cast arrives with [`CastEvent::prerolled`] set.
    fn trigger_spell(
        &mut self,
        caster: ActorId,
        spell: SpellId,
        target: Option<ActorId>,
        delay: SimDuration,
        rolled: Option<Vec<RolledHit>>,
    );
    /// Stop `target`'s cast if it is interruptible. Returns whether one was
    /// stopped.
    fn interrupt(&mut self, target: ActorId) -> bool;
    /// Move a seat's actor `yards` at once (no-op for anyone else). It
    /// counts toward movement the seat owes, and closes distance to
    /// `toward` if that is an enemy.
    fn displace(&mut self, actor: ActorId, yards: f64, toward: Option<ActorId>);
    /// Pending timers are readable through [`StateView::timers`], but
    /// realistic observers can't tell what a token means. Anything a player
    /// could see coming (a sigil about to land) should be an aura with
    /// `on_expire` effects instead.
    fn schedule(&mut self, delay: SimDuration, timer: TimerEvent);
    /// Uniform in `[0, 1)` from the actor's named stream.
    fn roll(&mut self, actor: ActorId, stream: StreamKey) -> f64;
    /// During a travelling spell's [`Mechanics::cast_completed`]: a hit
    /// rolled now, handed back by [`Mechanics::projectile_landed`] when the
    /// projectile lands (SimC rolls direct damage at execute). Dropped if
    /// the spell never launches.
    fn stash_hit(&mut self, hit: RolledHit);
    fn wake(&mut self, seat: Seat, reason: WakeReason);
}

/// A cast by a seat or one of its pets.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CastEvent {
    /// The seat, or the pet's owner.
    pub seat: Seat,
    /// Who is casting: the seat's actor or a pet.
    pub actor: ActorId,
    /// The ability pressed, before aura overrides; `None` for pet casts and
    /// triggered spells.
    pub ability: Option<SpellId>,
    /// What was actually cast.
    pub spell: SpellId,
    pub target: Option<ActorId>,
    pub started: SimTime,
    /// For empowers, the stage released at.
    pub empower: Option<u8>,
    /// What the spell's scaling cost actually consumed (see
    /// `SpendScaling`), else what its variable cost (one with `extra`) did.
    pub spent: Option<ResourceAmount>,
    /// A triggered spell whose direct damage was rolled when it was
    /// triggered (see [`EngineIo::trigger_spell`]): don't roll it again;
    /// the hits arrive with its landing.
    pub prerolled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwingEvent {
    pub actor: ActorId,
    pub hand: WeaponHand,
    pub target: ActorId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TickEvent {
    pub aura: AuraRef,
    pub index: u32,
    /// `1.0` for a full tick; less for a partial final tick.
    pub fraction: f64,
    /// Ticks still owed counting this one, a partial final tick by its
    /// fraction, at the source's current haste (SimC's
    /// `ticks_left_fractional`); `None` for a permanent aura.
    pub ticks_left: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuraChange {
    pub aura: AuraRef,
    /// 0 for a fresh application.
    pub previous_stacks: u8,
    pub stacks: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuraEvent {
    pub aura: AuraRef,
    pub reason: AuraRemoval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuraRemoval {
    Expired,
    Removed,
    HolderDied,
    /// Stealth broken by a cast or damage taken.
    Broken,
    /// An absorb used up.
    Depleted,
}

/// A valued aura's limits, evaluated by mechanics.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ValueLimits {
    /// Clamp the value to this after adding.
    pub cap: Option<f64>,
    /// After the cap, while the value is at least this, drop it by this
    /// and call [`Mechanics::aura_threshold`]. Must be positive.
    pub threshold: Option<f64>,
}

/// What a hit did: health removed (overkill included) and damage that
/// shields absorbed. A hit that connected has one of them non-zero, or
/// was a lethal hit an aura's `prevents_death` stopped; one fully absorbed
/// still counts as a hit for procs, as in SimC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Landed {
    pub amount: u64,
    pub absorbed: u64,
    pub prevented: bool,
}

impl Landed {
    pub fn connected(self) -> bool {
        self.amount > 0 || self.absorbed > 0 || self.prevented
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbsorbEvent {
    /// The shield: its holder took the hit.
    pub aura: AuraRef,
    pub attacker: ActorId,
    pub amount: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeathEvent {
    pub actor: ActorId,
    /// Whoever dealt the killing blow, if anyone did.
    pub killer: Option<ActorId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PetEvent {
    pub owner: Seat,
    pub pet: PetId,
    pub actor: ActorId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnemyHit {
    pub source: ActorId,
    pub target: ActorId,
    pub rule: RuleIndex,
    pub amount: f64,
    pub school: SchoolMask,
}

/// A computed hit, heal, or health pool as whole points, the way the game
/// keeps them: the nearest integer, with negatives and NaN as 0.
pub fn whole_points(amount: f64) -> u64 {
    if amount.is_nan() || amount <= 0.0 {
        0
    } else {
        amount.round() as u64
    }
}

/// A mechanics-owned timer; `token` means whatever the owner's spec kit
/// says it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerEvent {
    pub owner: ActorId,
    pub token: u32,
}

/// Whether a hit is direct or an aura's periodic tick (SimC's
/// `result_amount_type`). Armor reduces only direct physical hits, so
/// bleeds such as Rend and Rupture tick at full strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HitKind {
    Direct,
    Periodic,
}

/// A hit rolled before the target's mitigation: direct damage rolled when
/// its spell launched, to land with it, or any hit about to be delivered.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RolledHit {
    pub source: ActorId,
    pub target: ActorId,
    /// Crit included, before the target's mitigation.
    pub amount: f64,
    pub school: SchoolMask,
    pub spell: Option<SpellId>,
    pub crit: bool,
    /// The listener depth it was rolled at.
    pub depth: u8,
    pub kind: HitKind,
    /// A direct physical hit that bypasses armor anyway (see
    /// `Effect::Damage::ignores_armor`).
    pub ignores_armor: bool,
    /// The hand a strike hit with (see `Effect::Damage::hand` and
    /// `SpellDef::weapon`), whose weapon-hit listeners it fires on landing.
    pub weapon: Option<WeaponHand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DamageEvent {
    pub source: ActorId,
    pub target: ActorId,
    /// Whole points: mechanics round each hit (see [`whole_points`]).
    pub amount: u64,
    pub school: SchoolMask,
    pub spell: Option<SpellId>,
    pub crit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HealEvent {
    pub source: ActorId,
    pub target: ActorId,
    /// Whole points, like [`DamageEvent::amount`].
    pub amount: u64,
    pub spell: Option<SpellId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AuraApplication {
    pub aura: AuraRef,
    pub stacks: u8,
    /// Override the data duration (e.g. a talent-extended buff).
    pub duration: Option<SimDuration>,
    /// Override the snapshot taken now, for a new instance copying another
    /// (a spread aura).
    pub pmultiplier: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::whole_points;

    #[test]
    fn hits_round_to_the_nearest_point() {
        assert_eq!(whole_points(1234.49), 1234);
        assert_eq!(whole_points(1234.5), 1235);
        assert_eq!(whole_points(0.4), 0);
        assert_eq!(whole_points(-3.0), 0);
        assert_eq!(whole_points(f64::NAN), 0);
    }
}
