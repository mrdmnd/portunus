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
    EnemyRule {
        actor: ActorId,
        rule: RuleIndex,
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
