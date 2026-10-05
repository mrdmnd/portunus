//! Spec behavior.
//!
//! The party-level [`portunus_engine::Mechanics`] implementation,
//! [`PartyMechanics`], is built from three parts:
//!
//! - an [`EffectInterpreter`] that runs data [`Effect`]s generically
//!   ([`Interpreter`]);
//! - [`CombatMath`]: stat, modifier, crit, and mitigation formulas
//!   ([`Formulas`]);
//! - one [`SpecKit`] per spec, for the [`HookKey`]s and gates that data
//!   can't express ([`Kits`] holds them).
//!
//! Most behavior should be data; kits are the escape hatch, and they hold no
//! state of their own (see the rule in `portunus_engine::mechanics`).

mod interp;
pub mod kits;
mod math;
mod party;

use portunus_core::{ActorId, HookKey, Seat, SpecId, SpellId};
use portunus_engine::mechanics::TimerEvent;
use portunus_engine::{AuraRef, EngineIo, Readiness, StateView};
use portunus_gamedata::effect::{Coefficient, Effect};
use portunus_gamedata::stats::SchoolMask;
use portunus_gamedata::GameData;

pub use interp::Interpreter;
pub use kits::Kits;
pub use math::Formulas;
pub use party::PartyMechanics;

/// Where an effect list is running from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectCtx {
    pub caster: ActorId,
    pub target: Option<ActorId>,
    pub spell: Option<SpellId>,
    /// The aura instance whose tick, listener, expiry, or threshold is
    /// running; what `Coefficient::AuraValue` reads.
    pub aura: Option<AuraRef>,
    /// What `Coefficient::EventAmount` scales: the damage or healing a
    /// listener reacted to, the resource it saw spent, or a bank tick's
    /// draw.
    pub event_amount: Option<f64>,
    /// Multiplier on every amount: partial ticks, AoE falloff, and
    /// `SpendScaling`.
    pub scale: f64,
    /// How many listeners deep this run is. Effects run by a listener don't
    /// trigger listeners themselves, so procs can't feed each other.
    pub depth: u8,
}

pub trait EffectInterpreter {
    fn run(&self, io: &mut dyn EngineIo, ctx: &EffectCtx, effects: &[Effect]);
}

/// What an outgoing amount is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outgoing {
    Damage(SchoolMask),
    Heal,
}

pub trait CombatMath {
    /// The coefficient's raw value times `ctx.scale`, before any modifier.
    fn base(&self, view: &dyn StateView, ctx: &EffectCtx, amount: Coefficient) -> f64;
    /// Pre-crit amount after stats and every active modifier.
    fn outgoing(
        &self,
        view: &dyn StateView,
        ctx: &EffectCtx,
        amount: Coefficient,
        kind: Outgoing,
    ) -> f64;
    /// A probability in `[0, 1]`.
    fn crit_chance(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64;
    fn crit_multiplier(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64;
    /// Damage after the target's reductions (armor, versatility, defensives).
    fn mitigate(
        &self,
        view: &dyn StateView,
        target: ActorId,
        amount: f64,
        school: SchoolMask,
    ) -> f64;
    fn haste_mult(&self, view: &dyn StateView, actor: ActorId) -> f64;
}

/// The shared machinery a kit builds on, so hooks deal damage and run
/// effects exactly as data does.
#[derive(Clone, Copy)]
pub struct KitTools<'a> {
    pub effects: &'a dyn EffectInterpreter,
    pub math: &'a dyn CombatMath,
}

pub trait SpecKit: Send + Sync {
    fn spec(&self) -> SpecId;
    /// Every hook this kit implements; data naming any other is invalid.
    fn hooks(&self) -> &[HookKey];
    fn run_hook(&self, tools: KitTools<'_>, io: &mut dyn EngineIo, ctx: &EffectCtx, key: &HookKey);
    /// A timer this kit scheduled has fired; `token` is the kit's own.
    fn timer(&self, tools: KitTools<'_>, io: &mut dyn EngineIo, timer: &TimerEvent);
    fn gate(&self, view: &dyn StateView, seat: Seat, ability: SpellId) -> Readiness;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitIssue {
    /// A spec in the data has no kit.
    MissingKit(SpecId),
    /// Data names a hook that no kit implements.
    UnknownHook(HookKey),
    /// Two kits claim the same hook.
    DuplicateHook(HookKey),
}

pub trait SpecRegistry: Send + Sync {
    fn kit(&self, spec: SpecId) -> Option<&dyn SpecKit>;
    /// The kit implementing a hook, whichever spec is running it.
    fn hook_owner(&self, key: &HookKey) -> Option<&dyn SpecKit>;
    fn validate(&self, data: &GameData) -> Vec<KitIssue>;
}
