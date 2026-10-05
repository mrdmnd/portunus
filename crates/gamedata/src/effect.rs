//! The effect vocabulary: what spells, ticks, and procs *do*, as data.
//!
//! Grown additively. When something can't be said here, the data names a
//! [`HookKey`] and a spec kit implements it in code.

use portunus_core::{AuraId, HookKey, PetId, SimDuration, SpellId};
use serde::{Deserialize, Serialize};

use crate::item::WeaponHand;
use crate::stats::{RatedStat, ResourceAmount, ResourceKind, SchoolMask, Stat};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Damage {
        amount: Coefficient,
        school: SchoolMask,
        target: EffectTarget,
        #[serde(default)]
        aoe: Option<AoeRule>,
    },
    Heal {
        amount: Coefficient,
        target: EffectTarget,
    },
    ApplyAura {
        aura: AuraId,
        target: EffectTarget,
        #[serde(default = "crate::aura::one")]
        stacks: u8,
    },
    RemoveAura {
        aura: AuraId,
        target: EffectTarget,
    },
    /// Consume stacks without removing the aura (it goes when none remain).
    RemoveStacks {
        aura: AuraId,
        target: EffectTarget,
        stacks: u8,
    },
    /// Add time to an existing aura (no effect if absent).
    ExtendAura {
        aura: AuraId,
        target: EffectTarget,
        by: SimDuration,
    },
    /// Add to a valued aura (see `AuraValue`), applying it if absent.
    AddAuraValue {
        aura: AuraId,
        target: EffectTarget,
        amount: Coefficient,
    },
    /// Remove this fraction of a valued aura's value (e.g. Purifying Brew).
    ConsumeAuraValue {
        aura: AuraId,
        target: EffectTarget,
        fraction: f64,
    },
    /// Positive grants, negative drains (costs are gates, not effects).
    Resource(ResourceAmount),
    /// `duration` is ignored for permanent pets.
    Summon {
        pet: PetId,
        count: u8,
        duration: Option<SimDuration>,
    },
    /// Remove the caster's pets of this type, oldest first; `None` removes
    /// all (e.g. Implosion). Removal runs the pets' expiry like a timeout.
    Dismiss {
        pet: PetId,
        count: Option<u8>,
    },
    /// The caster's pets of this type (`None`: all of them) cast a spell at
    /// the cast's target (e.g. Kill Command), outside their autocast
    /// priority.
    CommandPet {
        pet: Option<PetId>,
        spell: SpellId,
    },
    /// Extend the caster's active guardians (e.g. Demonic Tyrant).
    ExtendPets {
        pet: Option<PetId>,
        by: SimDuration,
    },
    AdjustCooldown {
        spell: SpellId,
        change: CooldownChange,
    },
    /// Cast another spell for free (no gates), e.g. a proc's payload.
    TriggerSpell {
        spell: SpellId,
        target: EffectTarget,
    },
    /// Stop the target's cast if it is interruptible; its payload never
    /// lands.
    Interrupt {
        target: EffectTarget,
    },
    /// Run exactly one branch, chosen by weight from the caster's proc
    /// stream (e.g. one of three random buffs).
    RandomOf(Vec<(f64, Effect)>),
    If {
        when: Predicate,
        then: Vec<Effect>,
        #[serde(default)]
        otherwise: Vec<Effect>,
    },
    Hook(HookKey),
}

/// How an amount scales.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coefficient {
    Flat(f64),
    AttackPower(f64),
    SpellPower(f64),
    WeaponDamage(f64),
    PctMaxHealth(f64),
    /// A share of the triggering event's amount: the damage or healing a
    /// listener reacted to, the resource it saw spent, or the value a bank
    /// tick drew (e.g. Ignite banking a share of each crit).
    EventAmount(f64),
    /// A share of the current value of the valued aura running the effect.
    AuraValue(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectTarget {
    Caster,
    /// The cast's chosen target. For an aura's ticks, its holder; for a
    /// listener, the target of the event it reacted to.
    Target,
    /// Every engaged enemy (subject to the AoE rule).
    AllEnemies,
    Party,
    /// The caster's pets of this type (`None`: all of them).
    Pets(Option<PetId>),
    /// The pet's owner, for effects cast by a pet.
    Owner,
}

/// Target caps and damage reduction past a soft cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AoeRule {
    pub max_targets: Option<u8>,
    /// Damage scales by `sqrt(n / cap)` beyond this many targets.
    pub sqrt_cap: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CooldownChange {
    Reset,
    Reduce(SimDuration),
    /// Change the recovery rate, e.g. a 100% faster recharge.
    RateMult(f64),
    AddCharge,
}

/// A passive change to numbers, carried by an aura.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Modifier {
    pub scope: ModScope,
    pub kind: ModKind,
    /// In percent for `*Pct` kinds and `CritChanceAdd` (`10.0` is +10%, or
    /// +10 percentage points of crit); in stat points for `StatFlat`.
    pub value: f64,
    /// Multiply `value` by the aura's stack count.
    #[serde(default)]
    pub per_stack: bool,
    /// Applies only while this holds, e.g. "crits against targets with your
    /// Flame Shock".
    #[serde(default)]
    pub condition: Option<Predicate>,
}

/// A test against the current caster, target, and spell, used by
/// conditional modifiers and `Effect::If`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Predicate {
    /// The target has this aura; `from_self` requires the holder to be its
    /// source.
    TargetHasAura {
        aura: AuraId,
        from_self: bool,
    },
    CasterHasAura(AuraId),
    /// Below this fraction of max health (`0.2` is 20%), for execute
    /// windows.
    TargetHpBelow(f64),
    /// The spell differs from the caster's previous cast (Windwalker's
    /// combo strikes).
    DiffersFromLastCast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModScope {
    All,
    Spell(SpellId),
    School(SchoolMask),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModKind {
    DamageDonePct,
    DamageTakenPct,
    CritChanceAdd,
    CritDamagePct,
    HastePct,
    /// Auto-attack speed only.
    AttackSpeedPct,
    /// Periodic damage multiplier captured when an aura is applied and kept
    /// for its whole duration (a DoT's `pmultiplier`, e.g. Tiger's Fury on
    /// Rake). Live modifiers still apply on top.
    PersistentPct,
    CastTimePct,
    CooldownPct,
    CostPct,
    StatPct(Stat),
    StatFlat(Stat),
    /// Non-instant spells in scope become castable while moving (`value`
    /// unused).
    CastWhileMoving,
    /// Takes no damage in scope (`value` unused), e.g. an enemy's shield
    /// phase.
    Immune,
}

/// A reaction to combat events: procs, on-hit effects, and the like.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listener {
    pub on: ListenFor,
    pub chance: ProcChance,
    #[serde(default)]
    pub internal_cooldown: Option<SimDuration>,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenFor {
    /// `None` filters match anything.
    CastComplete {
        spell: Option<SpellId>,
        school: Option<SchoolMask>,
    },
    DamageDealt {
        spell: Option<SpellId>,
        school: Option<SchoolMask>,
        crit_only: bool,
    },
    DamageTaken,
    /// An auto-attack landed (poisons, Windfury). `None` matches both hands.
    Swing {
        hand: Option<WeaponHand>,
    },
    /// Each tick of this aura, from any holder, sourced by the listener's
    /// holder.
    PeriodicTick(AuraId),
    /// Fires per spend; effects run scaled by the amount spent.
    ResourceSpent(ResourceKind),
    AuraApplied(AuraId),
    AuraExpired(AuraId),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcChance {
    Always,
    /// A probability in `[0, 1]`.
    Flat(f64),
    /// Real procs-per-minute, with bad-luck protection.
    Rppm {
        rate: f64,
        hasted: bool,
    },
    /// Chance in percent is `coef` times a rated percentage, e.g.
    /// mastery-driven overloads (`coef: 1.0` at 20% mastery is a 20%
    /// chance).
    StatScaled {
        stat: RatedStat,
        coef: f64,
    },
}
