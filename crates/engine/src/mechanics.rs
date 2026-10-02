//! The plug-in point for game semantics.
//!
//! Rule: mechanics hold no mutable state of their own. Everything that
//! changes during a rollout lives in kernel primitives (auras, resources,
//! cooldowns, timers), which keeps forks cheap and all state observable.

use portunus_core::{
    AbilitySlot, ActorId, AuraId, PetId, Seat, SimDuration, SimTime, SpellId, StreamKey,
};
use portunus_gamedata::effect::CooldownChange;
use portunus_gamedata::enemy::RuleIndex;
use portunus_gamedata::item::WeaponHand;
use portunus_gamedata::stats::{ResourceAmount, ResourceKind, SchoolMask};
use portunus_gamedata::GameData;
use serde::{Deserialize, Serialize};

use crate::mask::Readiness;
use crate::state::{Projectile, StateView};
use crate::step::WakeReason;

pub trait Mechanics: Clone {
    /// Usability beyond the kernel's generic gates (cost, cooldown, GCD,
    /// casting, movement), e.g. "only during this proc".
    fn gate(&self, view: &dyn StateView, seat: Seat, slot: AbilitySlot) -> Readiness;
    fn cast_started(&self, io: &mut dyn EngineIo, cast: &CastEvent);
    fn cast_completed(&self, io: &mut dyn EngineIo, cast: &CastEvent);
    fn channel_tick(&self, io: &mut dyn EngineIo, cast: &CastEvent, tick: u8);
    /// A spell with travel time reached its target.
    fn projectile_landed(&self, io: &mut dyn EngineIo, cast: &CastEvent, flight: &Projectile);
    /// An auto-attack swing resolved.
    fn swing(&self, io: &mut dyn EngineIo, swing: &SwingEvent);
    fn periodic_tick(&self, io: &mut dyn EngineIo, tick: &TickEvent);
    fn aura_removed(&self, io: &mut dyn EngineIo, ev: &AuraEvent);
    /// An enemy rule hit a player: apply mitigation, then call
    /// [`EngineIo::apply_damage`].
    fn enemy_hit(&self, io: &mut dyn EngineIo, hit: &EnemyHit);
    fn timer(&self, io: &mut dyn EngineIo, timer: &TimerEvent);
}

/// What mechanics may do to the world. The kernel handles scheduling,
/// expiry, ticking, and wake-ups that follow from each call.
pub trait EngineIo {
    fn view(&self) -> &dyn StateView;
    fn data(&self) -> &GameData;
    fn apply_damage(&mut self, d: DamageEvent);
    fn apply_heal(&mut self, h: HealEvent);
    fn apply_aura(&mut self, a: AuraApplication);
    fn remove_aura(&mut self, holder: ActorId, aura: AuraId, source: Option<ActorId>);
    /// Push back an existing aura's expiry (no-op if absent).
    fn extend_aura(&mut self, holder: ActorId, aura: AuraId, source: ActorId, by: SimDuration);
    /// Change a valued aura's value, applying the aura if absent. The kernel
    /// enforces the cap and runs threshold effects.
    fn add_aura_value(&mut self, holder: ActorId, aura: AuraId, source: ActorId, delta: f64);
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
    /// Timers are invisible to observers. Anything a player could see
    /// coming (a sigil about to land, a bomb about to detonate) must be an
    /// aura with `on_expire` effects instead.
    fn schedule(&mut self, delay: SimDuration, timer: TimerEvent);
    /// Uniform in `[0, 1)` from the actor's named stream.
    fn roll(&mut self, actor: ActorId, stream: StreamKey) -> f64;
    fn wake(&mut self, seat: Seat, reason: WakeReason);
}

/// A cast by a seat or one of its pets.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CastEvent {
    /// The seat, or the pet's owner.
    pub seat: Seat,
    /// Who is casting: the seat's actor or a pet.
    pub actor: ActorId,
    /// `None` for pet casts and triggered spells.
    pub slot: Option<AbilitySlot>,
    pub spell: SpellId,
    pub target: Option<ActorId>,
    pub started: SimTime,
    /// For empowers, the stage released at.
    pub empower: Option<u8>,
    /// What the spell's scaling cost actually consumed (see
    /// `SpendScaling`), if it has one.
    pub spent: Option<ResourceAmount>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwingEvent {
    pub actor: ActorId,
    pub hand: WeaponHand,
    pub target: ActorId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TickEvent {
    pub holder: ActorId,
    pub source: ActorId,
    pub aura: AuraId,
    pub index: u32,
    /// `1.0` for a full tick; less for a partial final tick.
    pub fraction: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuraEvent {
    pub holder: ActorId,
    pub source: ActorId,
    pub aura: AuraId,
    pub reason: AuraRemoval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuraRemoval {
    Expired,
    Removed,
    HolderDied,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnemyHit {
    pub source: ActorId,
    pub target: ActorId,
    pub rule: RuleIndex,
    pub amount: f64,
    pub school: SchoolMask,
}

/// A mechanics-owned timer; `token` means whatever the owner's spec kit
/// says it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimerEvent {
    pub owner: ActorId,
    pub token: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DamageEvent {
    pub source: ActorId,
    pub target: ActorId,
    pub amount: f64,
    pub school: SchoolMask,
    pub spell: Option<SpellId>,
    pub crit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HealEvent {
    pub source: ActorId,
    pub target: ActorId,
    pub amount: f64,
    pub spell: Option<SpellId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuraApplication {
    pub holder: ActorId,
    pub source: ActorId,
    pub aura: AuraId,
    pub stacks: u8,
    /// Override the data duration (e.g. a talent-extended buff).
    pub duration: Option<SimDuration>,
}
