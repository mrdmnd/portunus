//! The event queue, ordered by `(time, class, sequence)`.
//!
//! Nothing is ever removed early. Events that a later change made stale
//! (a refreshed aura's old expiry, a stopped cast's completion) stay queued
//! and are recognized as stale when popped.

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use portunus_core::{ActorId, Seat, SimTime, SpellId};
use portunus_gamedata::enemy::RuleIndex;
use portunus_gamedata::item::WeaponHand;

use crate::mechanics::RolledHit;
use crate::order::EventClass;
use crate::state::AuraRef;
use crate::step::WakeReason;

use super::world::CooldownKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub time: SimTime,
    pub class: EventClass,
    pub seq: u64,
}

#[derive(Debug, Clone)]
pub(crate) enum Event {
    PrepullOpen,
    TravelEnd,
    CombatTimeout {
        combat: u16,
    },
    CastComplete {
        actor: ActorId,
        cast: u32,
    },
    /// Tick `index` (from 1) of a channel, if the actor is still
    /// channeling cast `cast`.
    ChannelTick {
        actor: ActorId,
        cast: u32,
        index: u8,
    },
    /// An empower reaches `stage` (from 1), if the actor is still charging
    /// cast `cast`.
    EmpowerStage {
        actor: ActorId,
        cast: u32,
        stage: u8,
    },
    ProjectileLand {
        id: u32,
    },
    Triggered {
        caster: ActorId,
        spell: SpellId,
        target: Option<ActorId>,
        rolled: Option<Vec<RolledHit>>,
    },
    AuraTick {
        aura: AuraRef,
        uid: u32,
    },
    AuraExpire {
        aura: AuraRef,
        uid: u32,
    },
    /// An independently timed stack may be dropping (`RefreshRule::Ironfur`).
    AuraStackExpire {
        aura: AuraRef,
        uid: u32,
    },
    Timer {
        id: u32,
    },
    EvalTriggers,
    CooldownReady {
        actor: ActorId,
        key: CooldownKey,
        gen: u32,
    },
    /// A pet may be able to cast from its autocast list.
    PetAct {
        actor: ActorId,
        gen: u32,
    },
    /// A guardian's or totem's lifetime may be up.
    PetExpire {
        actor: ActorId,
    },
    Swing {
        actor: ActorId,
        hand: WeaponHand,
        gen: u32,
    },
    /// An enemy rule fires, if its generation still matches.
    RuleFire {
        enemy: ActorId,
        rule: RuleIndex,
        gen: u32,
    },
    /// An enemy's cast bar may be complete.
    EnemyCastEnd {
        enemy: ActorId,
        seq: u32,
    },
    /// A seat may have reached its movement's goal or end.
    MovementEnd {
        seat: Seat,
        gen: u32,
    },
    /// A movement demand's time is up.
    DemandDeadline {
        seat: Seat,
        id: u32,
    },
    /// A seat's lagged cast is due to start, if it is still pending for
    /// this time.
    LaggedCast {
        seat: Seat,
    },
    /// An armed seat's predicted wake time arrived; check it still holds.
    Recheck {
        seat: Seat,
        gen: u32,
    },
    Deliver(Wake),
}

impl Event {
    fn class(&self) -> EventClass {
        match self {
            Event::TravelEnd
            | Event::CastComplete { .. }
            | Event::ChannelTick { .. }
            | Event::EmpowerStage { .. }
            | Event::ProjectileLand { .. }
            | Event::Triggered { .. }
            | Event::AuraTick { .. }
            | Event::AuraExpire { .. }
            | Event::AuraStackExpire { .. }
            | Event::Timer { .. }
            | Event::PetAct { .. }
            | Event::PetExpire { .. }
            | Event::Swing { .. }
            | Event::DemandDeadline { .. } => EventClass::World,
            Event::RuleFire { .. } | Event::EnemyCastEnd { .. } => EventClass::EnemyRules,
            Event::EvalTriggers => EventClass::Triggers,
            Event::PrepullOpen
            | Event::CombatTimeout { .. }
            | Event::CooldownReady { .. }
            | Event::MovementEnd { .. }
            | Event::LaggedCast { .. } => EventClass::Legality,
            Event::Recheck { .. } | Event::Deliver(_) => EventClass::Decisions,
        }
    }
}

/// A wake on its way to a seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wake {
    pub seat: Seat,
    pub reason: WakeReason,
    pub anticipated: bool,
    pub event_at: SimTime,
    /// Valid only while the seat's generation still equals this.
    pub gate: Option<u32>,
    /// The `unperceived` entry this delivery clears.
    pub perception: Option<u32>,
}

#[derive(Debug, Clone)]
struct Entry {
    key: Key,
    event: Event,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Queue {
    heap: BinaryHeap<Reverse<Entry>>,
    seq: u64,
}

impl Queue {
    pub fn push(&mut self, time: SimTime, event: Event) {
        let key = Key {
            time,
            class: event.class(),
            seq: self.seq,
        };
        self.seq += 1;
        self.heap.push(Reverse(Entry { key, event }));
    }

    pub fn peek_key(&self) -> Option<Key> {
        self.heap.peek().map(|Reverse(e)| e.key)
    }

    /// Whether any queued event matches, in no particular order.
    pub fn any(&self, f: impl Fn(&Event) -> bool) -> bool {
        self.heap.iter().any(|Reverse(e)| f(&e.event))
    }

    pub fn pop(&mut self) -> Option<(Key, Event)> {
        self.heap.pop().map(|Reverse(e)| (e.key, e.event))
    }
}
