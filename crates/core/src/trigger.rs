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

impl<S> Trigger<S> {
    /// The same trigger over another subject type.
    pub fn map<T>(&self, f: &mut impl FnMut(&S) -> T) -> Trigger<T> {
        match self {
            Trigger::Now => Trigger::Now,
            Trigger::Elapsed(d) => Trigger::Elapsed(*d),
            Trigger::HpFracBelow { who, frac } => Trigger::HpFracBelow {
                who: f(who),
                frac: *frac,
            },
            Trigger::AliveAtMost { who, count } => Trigger::AliveAtMost {
                who: f(who),
                count: *count,
            },
            Trigger::Died(who) => Trigger::Died(f(who)),
            Trigger::Fired { who, event, nth } => Trigger::Fired {
                who: f(who),
                event: event.clone(),
                nth: *nth,
            },
            Trigger::Delayed { delay, after } => Trigger::Delayed {
                delay: *delay,
                after: Box::new(after.map(f)),
            },
            Trigger::All(ts) => Trigger::All(ts.iter().map(|t| t.map(f)).collect()),
            Trigger::Any(ts) => Trigger::Any(ts.iter().map(|t| t.map(f)).collect()),
            Trigger::Never => Trigger::Never,
        }
    }

    /// Every subject mentioned, with the event name for `Fired`.
    pub fn visit(&self, f: &mut impl FnMut(&S, Option<&EventName>)) {
        match self {
            Trigger::Now | Trigger::Elapsed(_) | Trigger::Never => {}
            Trigger::HpFracBelow { who, .. }
            | Trigger::AliveAtMost { who, .. }
            | Trigger::Died(who) => f(who, None),
            Trigger::Fired { who, event, .. } => f(who, Some(event)),
            Trigger::Delayed { after, .. } => after.visit(f),
            Trigger::All(ts) | Trigger::Any(ts) => ts.iter().for_each(|t| t.visit(f)),
        }
    }
}
