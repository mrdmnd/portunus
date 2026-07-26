//! Simultaneity tie-break classes (invariant 4).
//!
//! Event ordering key is `(time, priority, seq)`. These classes are
//! gameplay-semantic decisions, documented in `docs/PRIORITY_CLASSES.md`,
//! defined once here and never bypassed. Lower discriminant = processed
//! first at equal time.

use serde::{Deserialize, Serialize};

/// Priority class of an event at equal timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Priority {
    /// Aura expiry resolves before anything else at the same instant: a
    /// buff that ends exactly when a cast completes does NOT snapshot into it.
    AuraExpiry = 0,
    /// Periodic ticks land after expiries (an aura expiring at tick time
    /// does not grant a bonus final tick) but before cast completions.
    PeriodicTick = 1,
    /// Cast completions (and their effect application).
    CastComplete = 2,
    /// Legality transitions: cooldown ready, GCD end, resource threshold
    /// crossings. After completions so a completion at time T is visible to
    /// the decision these wakes produce.
    LegalityChange = 3,
    /// Scripted encounter events (intake, demands).
    Encounter = 4,
    /// Policy-requested wakes (`Wait::Until`, predicate wakes).
    ScheduledWake = 5,
    /// Bookkeeping: combat timeout and other engine housekeeping. Last, so
    /// that same-instant gameplay resolution wins over, e.g., a timeout.
    Bookkeeping = 6,
}
