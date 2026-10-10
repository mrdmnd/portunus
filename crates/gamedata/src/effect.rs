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
        /// A direct physical hit that armor doesn't reduce, as it never
        /// reduces periodic damage (SimC's `ignores_armor`: Windstrike,
        /// Touch of Death, Shattering Throw).
        #[serde(default)]
        ignores_armor: bool,
        /// A strike with this hand, whatever the spell's own `weapon`: it
        /// scales with that hand's weapon and fires its `WeaponHit`
        /// listeners, and does nothing without a weapon there. Spells that
        /// strike with both hands (Stormstrike, Mutilate) have one effect
        /// for each.
        #[serde(default)]
        hand: Option<WeaponHand>,
        /// More damage per something counted when the effect runs (Mark of
        /// the Crane, Meat Cleaver).
        #[serde(default)]
        per_count: Option<CountScale>,
        /// The amount as given: no caster damage modifiers, versatility or
        /// snapshot, and no crit. SimC's `SX_DISABLE_PLAYER_MULT` with
        /// `SX_CANNOT_CRIT`, as on copies of damage already dealt (Blade
        /// Flurry's `trigger_residual_action`, with an `EventAmount`
        /// coefficient). The target's mitigation still applies unless
        /// `ignores_armor`.
        #[serde(default)]
        unmodified: bool,
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
        /// Instead of the aura's own, e.g. a talent-extended buff.
        #[serde(default)]
        duration: Option<SimDuration>,
        /// Added to the duration for each unit the cast spent (finishers:
        /// SimC's Rupture lasts its base times one plus its combo points).
        #[serde(default)]
        per_unit_spent: Option<SimDuration>,
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
    /// A grant whose amount scales, e.g. rage per second of weapon speed
    /// on each auto-attack.
    GainResource {
        kind: ResourceKind,
        amount: Coefficient,
    },
    /// The caster swings again at once with the weapon in play (the main
    /// hand if none): a full auto-attack whose own listeners don't fire
    /// (Skyfury).
    ExtraSwing,
    /// `duration` (or the pet's own, if `None`) is ignored for
    /// [`crate::pet::PetKind::Pet`].
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
    /// the cast's target for free, outside their autocast priority (Kill
    /// Command, or Ancestors echoing the owner's casts).
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
    /// After a `delay` it goes out then, with damage rolled now unless it
    /// `rolls_on_impact` (an overload, 400 ms after its parent).
    TriggerSpell {
        spell: SpellId,
        target: EffectTarget,
        #[serde(default)]
        delay: SimDuration,
    },
    /// Stop the target's cast if it is interruptible; its payload never
    /// lands.
    Interrupt {
        target: EffectTarget,
    },
    /// Move the caster `yards` at once (Blink, Gust of Wind). It counts
    /// toward movement the caster owes, and closes distance to the cast's
    /// target if `toward_target`.
    Displace {
        yards: f64,
        #[serde(default)]
        toward_target: bool,
    },
    /// Run exactly one branch, chosen by weight from the caster's proc
    /// stream (e.g. one of three random buffs).
    RandomOf(Vec<(f64, Effect)>),
    /// Run `then` with this probability, rolled from the caster's random
    /// stream (e.g. an extra overload that must not chain).
    Chance {
        chance: f64,
        then: Vec<Effect>,
    },
    /// Apply one of these auras, chosen evenly among those the target
    /// lacks, or among all of them if it has every one (Elemental Blast's
    /// buffs, which never repeat one already up).
    ApplyOneOf {
        auras: Vec<AuraId>,
        target: EffectTarget,
    },
    If {
        when: Predicate,
        then: Vec<Effect>,
        #[serde(default)]
        otherwise: Vec<Effect>,
    },
    /// Run `then` once per target, as if each were the event's target.
    ForEach {
        target: EffectTarget,
        then: Vec<Effect>,
    },
    Hook(HookKey),
    /// Copy the caster's `aura` on the event's target to up to `max` of
    /// `to` that don't have the caster's yet: the same stacks and
    /// snapshot, for the time it has left, ticking afresh. SimC's
    /// `dot_t::copy` with `DOT_COPY_START` (disease spread, Contagion).
    SpreadAura {
        aura: AuraId,
        to: EffectTarget,
        #[serde(default)]
        max: Option<u8>,
    },
}

/// Multiplies an amount by `1 + pct / 100 * count`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CountScale {
    pub count: TargetCount,
    pub pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetCount {
    /// Engaged enemies holding `aura` (from the caster, if `from_self`).
    EnemiesWithAura { aura: AuraId, from_self: bool },
    /// The targets this effect hits.
    TargetsHit,
}

/// How an amount scales.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coefficient {
    Flat(f64),
    AttackPower(f64),
    SpellPower(f64),
    /// A white hit's worth (before modifiers) with the weapon in play, as
    /// for `WeaponSpeed`.
    WeaponDamage(f64),
    PctMaxHealth(f64),
    /// A share of the triggering event's amount: the damage or healing a
    /// listener reacted to, the resource it saw spent, or the value a bank
    /// tick drew (e.g. Ignite banking a share of each crit).
    EventAmount(f64),
    /// A share of the current value of the valued aura running the effect.
    AuraValue(f64),
    /// Per second of the unhasted speed of the weapon in play: the swing's
    /// or strike's that triggered the effect, else the main hand.
    WeaponSpeed(f64),
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
    /// One engaged enemy, chosen evenly from the caster's random stream
    /// (none out of combat).
    RandomEnemy,
    /// Engaged enemies holding `aura` (from the caster, if `from_self`):
    /// Malefic Rapture.
    EnemiesWithAura {
        aura: AuraId,
        from_self: bool,
    },
    /// Every engaged enemy but the target: cleave, damage copying.
    OtherEnemies,
    /// Seats holding `aura` (from the caster, if `from_self`): Atonement.
    AlliesWithAura {
        aura: AuraId,
        from_self: bool,
    },
    /// The target and every engaged enemy within `yards` of it, by pack
    /// position: an area centred on the target. As SimC's
    /// `action_t::check_distance_targeting` does with distance targeting
    /// on, the target always counts and others count up to and including
    /// the radius.
    NearTarget {
        yards: u8,
    },
}

/// Target caps and damage reduction past a soft cap.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AoeRule {
    pub max_targets: Option<u8>,
    /// Beyond this many targets, each takes `sqrt(cap / n)` of the damage.
    pub sqrt_cap: Option<u8>,
    /// The share of the damage that targets other than the cast's target
    /// take, in `[0, 1]` (SimC's `base_aoe_multiplier`, e.g. Tempest's
    /// 65%).
    #[serde(default = "full_share")]
    pub secondary: f64,
}

fn full_share() -> f64 {
    1.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CooldownChange {
    Reset,
    /// Start a full recovery from now, using a charge if all are ready: a
    /// spell whose cooldown waits for its buff to be spent (Ancestral
    /// Swiftness, "cooldown on event").
    Restart,
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
    CasterLacksAura(AuraId),
    /// For a pet's effects: its owner has this aura, e.g. the owner's
    /// talent changing what the pet does. False for non-pets.
    OwnerHasAura(AuraId),
    /// Below this fraction of max health (`0.2` is 20%), for execute
    /// windows.
    TargetHpBelow(f64),
    /// The spell differs from the caster's previous cast (Windwalker's
    /// combo strikes). A cast's own effects see the casts before it, as in
    /// SimC's `is_combo_strike`.
    DiffersFromLastCast,
    /// Above this fraction of max health (Careful Aim, Firestarter).
    TargetHpAbove(f64),
    /// The caster is below this fraction of its max health (defensive
    /// talents like Second Wind).
    CasterHpBelow(f64),
    /// The caster is above this fraction of its max health.
    CasterHpAbove(f64),
    /// The target has at least this many stacks of the aura; `from_self`
    /// counts only the caster's instance ("at 5 stacks of Festering
    /// Wound").
    TargetStacksAtLeast {
        aura: AuraId,
        stacks: u8,
        from_self: bool,
    },
    /// The caster has at least this many stacks of the aura.
    CasterStacksAtLeast {
        aura: AuraId,
        stacks: u8,
    },
    /// The caster's instance of the aura carries at least this value
    /// (counters; see `AuraValue`).
    AuraValueAtLeast {
        aura: AuraId,
        value: f64,
    },
    /// The target is alive with no more health than the caster's maximum
    /// (Touch of Death; SimC compares `current_health() <= max_health()`).
    TargetHpBelowCasterMaxHp,
    /// The caster's last `count` casts were all this spell (Steady Focus:
    /// two Steady Shots in a row). A cast's own effects see the casts
    /// before it. `count` is from 1 to [`RECENT_CASTS`].
    RecentCasts {
        spell: SpellId,
        count: u8,
    },
    /// The caster stands in its own placement of this ground aura ("while
    /// standing in your Death and Decay"). Movement has no direction, so
    /// the caster counts as inside until it has moved, in all, more than
    /// the radius since placing it. Pets and totems never leave.
    InOwnGround(AuraId),
}

/// How many of a seat's casts the engine remembers for
/// [`Predicate::RecentCasts`].
pub const RECENT_CASTS: usize = 4;

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
    /// On an owner's aura: damage done by its [`crate::pet::PetKind::Pet`]s,
    /// scoped by the pet's spell (SimC's pet damage multiplier).
    PetDamagePct,
    /// On an owner's aura: damage done by its guardians, scoped by the
    /// guardian's spell. Totems use the owner's `DamageDonePct` instead.
    GuardianDamagePct,
    DamageTakenPct,
    CritChanceAdd,
    CritDamagePct,
    /// Multiplies haste from rating (Bloodlust, Unlimited Power). A seat's
    /// reaches its pets; on a pet's own aura, it stacks with the owner's
    /// haste the pet inherits.
    HastePct,
    /// Auto-attack speed only, on top of haste.
    AttackSpeedPct,
    /// Periodic damage multiplier captured when an aura is applied and kept
    /// for its whole duration (a DoT's `pmultiplier`, e.g. Tiger's Fury on
    /// Rake). Live modifiers still apply on top.
    PersistentPct,
    CastTimePct,
    /// The GCD a spell in scope triggers, before its floor (Windspeaker).
    GcdPct,
    CooldownPct,
    /// Extra charges for a spell in scope, as a whole number (Elemental
    /// Reverb). Read when the cooldown is first used.
    ChargesAdd,
    CostPct,
    /// A seat's maximum of this resource, in points (Primordial Capacity).
    /// Read from the seat's passive auras at the start of the run; `scope`
    /// is unused.
    ResourceMax(ResourceKind),
    StatPct(Stat),
    StatFlat(Stat),
    /// A seat's attack power, after it is derived from primary stats
    /// (Battle Shout); `scope` is unused.
    AttackPowerPct,
    /// Points added to a rated secondary after rating conversion. Mastery
    /// points are scaled by the spec's mastery coefficient, as in game
    /// (Storm Swell's "Mastery +5").
    RatedPct(RatedStat),
    /// Non-instant spells in scope become castable while moving (`value`
    /// unused).
    CastWhileMoving,
    /// Run speed, in percent; `scope` is unused.
    MoveSpeedPct,
    /// Takes no damage in scope (`value` unused), e.g. an enemy's shield
    /// phase.
    Immune,
    /// Damage in scope is multiplied by the caster's crit chance, uncapped
    /// (`value` unused). Paired with a +100 `CritChanceAdd`, a spell always
    /// crits and grows with crit past that (Farseer's Ancestors' Lava
    /// Burst, SimC's `base_crit = 1.0` times `composite_crit_chance`).
    CritChanceScalesDamage,
    /// Damage in scope is multiplied by one plus the caster's crit chance,
    /// leaving out crit granted against particular targets (`value`
    /// unused): Elemental's Lava Burst, whose Flame Shock crit doesn't
    /// count.
    CritChanceAddsDamage,
}

/// A reaction to combat events: procs, on-hit effects, and the like.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Listener {
    pub on: ListenFor,
    pub chance: ProcChance,
    #[serde(default)]
    pub internal_cooldown: Option<SimDuration>,
    /// For `ResourceSpent` only: roll once per whole unit spent instead of
    /// once per spend, and proc at most once (SimC's per-Maelstrom decks,
    /// e.g. Tempest). The effects then run once, unscaled.
    #[serde(default)]
    pub per_unit: bool,
    /// Draw from an earlier listener's proc bookkeeping on the same aura
    /// (its deck, stream, and internal cooldown) instead of keeping its
    /// own: one SimC `shuffled_rng` rolled from several places, e.g.
    /// Routine Communication. The chances must match.
    #[serde(default)]
    pub shared_with: Option<u8>,
    /// Only reacts while this holds, checked before rolling, with the
    /// holder as caster and the event's target as target.
    #[serde(default)]
    pub condition: Option<Predicate>,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenFor {
    /// A cast succeeding: hard casts as they complete, channels as they
    /// start (the game's `SPELL_CAST_SUCCESS`), empowers as they're
    /// released. `None` filters match anything.
    CastComplete {
        spell: Option<SpellId>,
        school: Option<SchoolMask>,
    },
    DamageDealt {
        spell: Option<SpellId>,
        school: Option<SchoolMask>,
        crit_only: bool,
        /// Only hits that kill their target.
        #[serde(default)]
        killing_blow: bool,
    },
    DamageTaken,
    /// An auto-attack landed (rage, Windfury). `None` matches both hands.
    Swing {
        hand: Option<WeaponHand>,
    },
    /// A melee hit landed: an auto-attack, or a hit from a spell with a
    /// `weapon` (poisons). `None` matches both hands.
    WeaponHit {
        hand: Option<WeaponHand>,
    },
    /// Each tick of this aura, from any holder, sourced by the listener's
    /// holder.
    PeriodicTick(AuraId),
    /// Fires per spend; effects run scaled by the amount spent.
    ResourceSpent(ResourceKind),
    AuraApplied(AuraId),
    AuraExpired(AuraId),
    /// One of the holder's pets of this type reached the end of its
    /// lifetime (not dismissed or replaced), e.g. an Ancestor departing.
    /// The event's target is the holder's.
    PetExpired(PetId),
    /// The holder, a pet, reached the end of its lifetime. It has stopped
    /// acting but can still cast from here (an Ancestor's parting
    /// Elemental Blast); fires before its owner's `PetExpired`. The event's
    /// target is the owner's.
    Departed,
    /// An enemy died (not one that left with its spawner). Fires on every
    /// seat's and pet's auras; the event's target is the dead enemy.
    EnemyDied {
        /// Only if the listener's holder dealt the killing blow.
        #[serde(default)]
        killed_by_self: bool,
        /// Only if the enemy died with this aura from the holder.
        #[serde(default)]
        had_aura: Option<AuraId>,
    },
    /// A cast with a cast time began (the game's `SPELL_CAST_START`):
    /// hard casts and empowers, not instants or channels. A proc that
    /// makes a hard cast instant still counts.
    CastStart {
        spell: Option<SpellId>,
        school: Option<SchoolMask>,
    },
    /// The holder gained some of this resource from an effect, with the
    /// amount gained (after the cap) as the event's amount. Regeneration
    /// and gains a spec kit grants directly don't count.
    ResourceGained(ResourceKind),
    /// This aura, cast by the holder, absorbed damage on whoever holds
    /// it: fires on the shield's caster, with the amount absorbed. The
    /// event's target is the shielded actor.
    Absorbed(AuraId),
    /// The holder's effect stopped a cast. The event's target is the
    /// interrupted actor.
    Interrupted,
    /// The holder, a seat, started moving from standing still.
    MoveStart,
    /// The holder, a seat, stopped moving.
    MoveEnd,
    /// A hit took the holder's health from at least `pct`% of its maximum
    /// to below it. The event's target is the attacker.
    HealthBelow {
        pct: u8,
    },
    /// The holder's `aura` (from anyone) reached `stacks` from fewer.
    AuraStacksReached {
        aura: AuraId,
        stacks: u8,
    },
    /// The holder lost `aura` this way (never by its own death).
    AuraRemovedBy {
        aura: AuraId,
        reason: RemovalReason,
    },
}

/// Why an aura went, for listeners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemovalReason {
    /// It ran out.
    Expired,
    /// Something removed or consumed it.
    Removed,
    /// An absorb used up.
    Depleted,
    /// Stealth broken by a cast or damage taken.
    Broken,
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
    /// A shuffled deck, as SimC's `shuffled_rng`: exactly `successes` of
    /// every `size` attempts proc, in random order, and the deck reshuffles
    /// once every card is drawn. Each holder keeps its deck for the whole
    /// run (in game, until logout), even across reapplications of the aura.
    Deck {
        successes: u16,
        size: u16,
    },
}
