//! Recorded events, for debugging, replay, and timeline views.

use portunus_core::{ActorId, AuraId, PetId, Seat, SimTime, SpellId};
use portunus_gamedata::enemy::RuleIndex;
use serde::{Deserialize, Serialize};

use crate::choice::Choice;
use crate::mechanics::{DamageEvent, HealEvent};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceRecord {
    pub time: SimTime,
    pub event: TraceEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceEvent {
    CombatStart {
        combat: u16,
    },
    CombatEnd {
        combat: u16,
        cleared: bool,
    },
    Engage {
        actor: ActorId,
    },
    Death {
        actor: ActorId,
    },
    Decision {
        seat: Seat,
        choice: Choice,
    },
    CastStart {
        actor: ActorId,
        spell: SpellId,
        target: Option<ActorId>,
    },
    CastEnd {
        actor: ActorId,
        spell: SpellId,
        reason: CastEndReason,
    },
    /// Tick `index` (from 1) of a channel.
    ChannelTick {
        actor: ActorId,
        spell: SpellId,
        index: u8,
    },
    /// An empower reached `stage` (from 1).
    EmpowerStage {
        actor: ActorId,
        spell: SpellId,
        stage: u8,
    },
    EnemyRule {
        actor: ActorId,
        rule: RuleIndex,
    },
    EnemyCastStart {
        actor: ActorId,
        rule: RuleIndex,
        ends: SimTime,
    },
    EnemyCastEnd {
        actor: ActorId,
        rule: RuleIndex,
        reason: CastEndReason,
    },
    MovementStart {
        actor: ActorId,
        ends: SimTime,
        forced: bool,
    },
    MovementEnd {
        actor: ActorId,
    },
    /// `actor` owes `yards` of movement by `deadline`.
    Demand {
        actor: ActorId,
        yards: f64,
        deadline: SimTime,
    },
    DemandMet {
        actor: ActorId,
    },
    DemandFailed {
        actor: ActorId,
    },
    /// The enemy `actor` is now `distance` yards from `seat`.
    Distance {
        actor: ActorId,
        seat: Seat,
        distance: f64,
    },
    Resurrect {
        actor: ActorId,
    },
    Damage(DamageEvent),
    Heal(HealEvent),
    AuraApplied {
        holder: ActorId,
        aura: AuraId,
        stacks: u8,
    },
    AuraRemoved {
        holder: ActorId,
        aura: AuraId,
    },
    PetSummoned {
        owner: Seat,
        pet: PetId,
        actor: ActorId,
    },
    PetExpired {
        actor: ActorId,
    },
    ProjectileLaunched {
        actor: ActorId,
        spell: SpellId,
        target: ActorId,
        lands: SimTime,
    },
    /// `aura` on `target` soaked `amount` of a hit from `source`, before
    /// the rest (if any) was recorded as [`TraceEvent::Damage`].
    Absorbed {
        source: ActorId,
        target: ActorId,
        aura: AuraId,
        amount: u64,
    },
    /// An enemy rule brought `actor` into the combat.
    Spawn {
        actor: ActorId,
        spawner: ActorId,
    },
    /// `actor` left the combat without dying: its spawner died.
    Despawn {
        actor: ActorId,
    },
    /// `aura` kept `actor` alive through a lethal hit.
    DeathPrevented {
        actor: ActorId,
        aura: AuraId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CastEndReason {
    Completed,
    /// The caster chose `StopCast`.
    Stopped,
    /// Forced movement, an interrupt, or the caster's death.
    Interrupted,
}
