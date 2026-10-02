//! Canonical ordering of events that share a timestamp.
//!
//! The queue orders by `(time, class, sequence)`. Decisions come last, so a
//! policy never sees a half-resolved moment. Reordering variants changes
//! results and breaks determinism against recorded traces.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum EventClass {
    /// Damage, heals, aura ticks and expiries, cast and channel completions.
    World = 0,
    /// Enemy rules whose triggers fired.
    EnemyRules = 1,
    /// Engagement and phase triggers, re-evaluated until nothing new fires.
    Triggers = 2,
    /// GCD end, cooldown and charge returns, movement end.
    Legality = 3,
    /// Condition-wait rechecks, then delivery of the decision batch.
    Decisions = 4,
}
