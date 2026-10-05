//! What a policy can answer at a decision point.

use portunus_core::{ActorId, AuraId, SimTime, SpellId};
use portunus_gamedata::stats::ResourceKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    Cast {
        /// One of the seat's abilities, by the id it was granted under; aura
        /// overrides change what it casts, not this key.
        ability: SpellId,
        target: TargetSel,
        opts: CastOpts,
    },
    Wait(Wait),
    /// Abandon the current cast or channel. A cast's effects never land; a
    /// channel keeps the ticks it already dealt. Cooldowns and costs follow
    /// game rules for cancelled spells. The seat is free afterwards and is
    /// asked again at the same timestamp, so "stop, then interrupt" is two
    /// decisions with no time between them. Clipping a channel and
    /// chaining into a new cast is `StopCast` followed by `Cast`.
    StopCast,
    /// Change the primary target (what `TargetSel::Primary` means, and
    /// what auto-attacks and pets hit). Free; the seat is asked again at the
    /// same timestamp.
    SetTarget(ActorId),
    /// Remove one of this seat's own cancelable auras. Free; the seat is
    /// asked again at the same timestamp.
    CancelAura(AuraId),
}

/// Per-cast options. The default is a plain cast.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastOpts {
    /// For empowered spells, release automatically on reaching this stage;
    /// `None` charges to the final stage and holds until the hold runs out.
    /// The seat can also release early with `StopCast`, which fires the
    /// stage reached so far.
    pub empower: Option<u8>,
    /// For channels, wake the seat at every tick (`Wait::ChannelTick`
    /// covers a single tick).
    pub tick_wakes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetSel {
    /// The current primary target.
    Primary,
    Actor(ActorId),
    LowestHealth,
    HighestHealth,
    /// An enemy casting something interruptible.
    Casting,
}

/// Every wait is interruptible: the seat's default wake reasons (procs,
/// engagements, deaths, forced movement) still fire. Waits name an absolute
/// moment or a condition, never a relative duration, so haste changes can't
/// make them drift.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wait {
    /// Until the next decision-relevant event.
    NextEvent,
    Until(SimTime),
    GcdEnd,
    /// Until the current channel's next tick. Valid only while channeling,
    /// and it keeps the channel going: the seat wakes right after the tick
    /// resolves, free to clip or continue.
    ChannelTick,
    /// Until a condition over the seat's own state becomes true. Linear
    /// quantities (resources, timers) are solved exactly, not polled.
    Condition(Condition),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Cmp { lhs: Scalar, op: CmpOp, rhs: f64 },
    All(Vec<Condition>),
    Any(Vec<Condition>),
}

/// Durations and times are in milliseconds, like everywhere else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scalar {
    Resource(ResourceKind),
    /// Until ready (0 if ready).
    CooldownRemaining(SpellId),
    Charges(SpellId),
    /// Remaining on an aura this seat holds (0 if absent).
    AuraRemaining(AuraId),
    AuraStacks(AuraId),
    /// The value carried by an aura this seat holds (0 if absent).
    AuraValue(AuraId),
    /// Remaining on this seat's aura on its primary target (0 if absent).
    TargetAuraRemaining(AuraId),
    /// Since the start of the run.
    Time,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmpOp {
    Lt,
    Le,
    Ge,
    Gt,
}
