//! The pending-event queue: a binary heap keyed `(SimTime, Priority, seq)`.
//!
//! The queue is part of the snapshotable state (invariant 7): `Clone` is the
//! snapshot. Invalidation is lazy — guarded events whose generation moved on
//! are discarded at pop (invariant 5).

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

use sim_types::SimTime;

use crate::event::{Event, EventPayload, GenGuard};
use crate::priority::Priority;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HeapEntry(Event);

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.0.time, self.0.priority, self.0.seq).cmp(&(
            other.0.time,
            other.0.priority,
            other.0.seq,
        ))
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Deterministically ordered pending-event queue.
#[derive(Debug, Clone, Default)]
pub struct EventScheduler {
    heap: BinaryHeap<Reverse<HeapEntry>>,
    next_seq: u64,
}

impl EventScheduler {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedule an event; returns its `seq` (useful for livelock tracking).
    pub fn schedule(
        &mut self,
        time: SimTime,
        priority: Priority,
        payload: EventPayload,
        guard: Option<GenGuard>,
    ) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.heap.push(Reverse(HeapEntry(Event { time, priority, seq, payload, guard })));
        seq
    }

    /// Pop the next event regardless of guard validity.
    pub fn pop(&mut self) -> Option<Event> {
        self.heap.pop().map(|Reverse(HeapEntry(e))| e)
    }

    /// Time of the earliest pending event, if any.
    #[must_use]
    pub fn peek_time(&self) -> Option<SimTime> {
        self.heap.peek().map(|Reverse(HeapEntry(e))| e.time)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.heap.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::ActorId;

    /// Invariant 4: pops are ordered by (time, priority, seq) regardless of
    /// insertion order.
    #[test]
    fn pop_order_is_time_priority_seq() {
        let mut s = EventScheduler::new();
        let a = ActorId(0);
        // Insert deliberately shuffled.
        s.schedule(200, Priority::CastComplete, EventPayload::GcdEnd { actor: a }, None);
        s.schedule(100, Priority::LegalityChange, EventPayload::GcdEnd { actor: a }, None);
        s.schedule(100, Priority::AuraExpiry, EventPayload::CombatTimeout { combat: 0 }, None);
        s.schedule(100, Priority::LegalityChange, EventPayload::CombatBegin { combat: 0 }, None);
        s.schedule(50, Priority::Bookkeeping, EventPayload::CombatTimeout { combat: 0 }, None);

        let popped: Vec<_> = std::iter::from_fn(|| s.pop()).collect();
        let keys: Vec<_> = popped.iter().map(|e| (e.time, e.priority, e.seq)).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert_eq!(popped[0].time, 50);
        // At t=100: AuraExpiry first, then the two LegalityChange in seq order.
        assert_eq!(popped[1].priority, Priority::AuraExpiry);
        assert!(popped[2].seq < popped[3].seq);
    }
}
