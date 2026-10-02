//! Spec behavior.
//!
//! The party-level [`portunus_engine::Mechanics`] implementation is built
//! from three parts:
//!
//! - an [`EffectInterpreter`] that runs data [`Effect`]s generically;
//! - [`CombatMath`]: stat, modifier, crit, and mitigation formulas;
//! - one [`SpecKit`] per spec, for the [`HookKey`]s and gates that data
//!   can't express.
//!
//! Most behavior should be data; kits are the escape hatch, and they hold no
//! state of their own (see the rule in `portunus_engine::mechanics`).

use portunus_core::{AbilitySlot, ActorId, AuraId, HookKey, Seat, SpecId, SpellId};
use portunus_engine::{EngineIo, Readiness, StateView};
use portunus_gamedata::effect::{Coefficient, Effect};
use portunus_gamedata::spec::Role;
use portunus_gamedata::stats::SchoolMask;
use portunus_gamedata::GameData;
use portunus_loadout::ActorTemplate;

/// Where an effect list is running from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectCtx {
    pub caster: ActorId,
    pub target: Option<ActorId>,
    pub spell: Option<SpellId>,
    pub aura: Option<AuraId>,
    /// Multiplier from context: partial ticks, AoE scaling.
    pub scale: f64,
}

pub trait EffectInterpreter {
    fn run(&self, io: &mut dyn EngineIo, ctx: &EffectCtx, effects: &[Effect]);
}

pub trait CombatMath {
    /// Pre-crit amount after stats and every active modifier.
    fn outgoing(
        &self,
        view: &dyn StateView,
        ctx: &EffectCtx,
        amount: Coefficient,
        school: SchoolMask,
    ) -> f64;
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

pub trait SpecKit: Send + Sync {
    fn spec(&self) -> SpecId;
    /// Every hook this kit implements; data naming any other is invalid.
    fn hooks(&self) -> &[HookKey];
    fn run_hook(&self, key: &HookKey, io: &mut dyn EngineIo, ctx: &EffectCtx);
    fn gate(&self, view: &dyn StateView, seat: Seat, slot: AbilitySlot) -> Readiness;
}

pub trait SpecRegistry: Send + Sync {
    fn kit(&self, spec: SpecId) -> Option<&dyn SpecKit>;
    /// Hooks named in data that no kit implements, and similar gaps.
    fn validate(&self, data: &GameData) -> Vec<String>;
}

/// Simple templates for party members whose roles aren't being optimized.
pub trait StandIns {
    fn template(&self, data: &GameData, role: Role) -> ActorTemplate;
}
