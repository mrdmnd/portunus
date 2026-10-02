//! The trigger language: conditions over combat state.
//!
//! One language serves every place that asks "has this happened yet?" —
//! when a wave engages, when an enemy rule fires, when a phase starts. It is
//! generic over the subject type `S`, so each use names only what it can
//! legally see: enemy rules talk about themselves, waves talk about spawns
//! in their pull, and the resolved form talks about spawn indices.

use serde::{Deserialize, Serialize};

use crate::ids::EventName;
use crate::time::SimDuration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger<S> {
    /// True as soon as the owner's clock starts.
    Now,
    /// The owner's clock (engagement for enemies, combat start for waves)
    /// has run this long.
    Elapsed(SimDuration),
    /// Remaining health of `who`, summed over a group, as a fraction of its
    /// summed max, is below `frac`.
    HpFracBelow {
        who: S,
        frac: f64,
    },
    /// At most `count` members of `who` are alive.
    AliveAtMost {
        who: S,
        count: u32,
    },
    /// Every member of `who` is dead.
    Died(S),
    /// `who` has fired `event` at least `nth` times (1-based).
    Fired {
        who: S,
        event: EventName,
        nth: u32,
    },
    /// `delay` after `after` first became true.
    Delayed {
        delay: SimDuration,
        after: Box<Trigger<S>>,
    },
    All(Vec<Trigger<S>>),
    Any(Vec<Trigger<S>>),
    Never,
}
