//! Actions, waits, and the engine-produced legality mask (SCAFFOLD.md §2).

use serde::{Deserialize, Serialize};

use crate::ids::{PredicateId, SlotId, TargetSlot};
use crate::time::SimTime;

/// Maximum ability slots per actor; matches the `castable_now` bitmask width.
pub const MAX_ACTIONS: usize = 64;

/// Target selection head. Only meaningful for slots whose codex definition
/// demands a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetSel {
    /// The current primary enemy (lowest actor id alive).
    Primary,
    /// The n-th enemy in the resolved pull.
    Slot(TargetSlot),
    /// The living enemy with the least absolute health.
    LowestHp,
    /// The enemy currently casting an interruptible spell.
    InterruptTarget,
}

/// What a policy may submit at a decision point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Begin a cast from an ability slot.
    Cast { slot: SlotId, target: TargetSel },
    /// Yield until a named future instant. All waits are interruptible
    /// leases: the actor's default decision-point triggers stay live.
    Wait(WaitSpec),
}

/// How a wait names its wake-up time.
///
/// NOTE deliberately absent: `Wait::For(duration)`. Relative waits are banned
/// (drift, haste-mutation bugs); every wait must name an absolute anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitSpec {
    /// Bare wait: wake on the next decision-relevant event. Submitting this
    /// twice on the same trigger is an engine-detected livelock error.
    NextEvent,
    /// Wake at an absolute sim time (must be strictly in the future).
    Until(SimTime),
    /// Wake when an engine-compiled predicate becomes true.
    UntilPredicate(PredicateId),
    /// Wake when the actor's current global cooldown ends.
    GcdEnd,
}

/// Engine-suggested semantic wait anchors, precomputed for the policy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitMenu {
    /// When the current GCD ends, if one is running.
    pub gcd_end: Option<SimTime>,
    /// Earliest future time any of this actor's cooldowns comes back up.
    pub next_cooldown_ready: Option<SimTime>,
    /// Time of the next event in the queue, whatever it is.
    pub next_scheduled_event: Option<SimTime>,
}

/// Per-decision legality mask produced by [`Engine::legal`].
///
/// `usable_in` enables micro-waiting: seconds until each slot becomes legal
/// (`f32::INFINITY` if unreachable under current rates and cooldowns).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActionMask {
    /// Bit `i` set iff slot `i` is castable right now.
    pub castable_now: u64,
    /// Seconds until each slot becomes legal; `0.0` if castable now.
    pub usable_in: [f32; MAX_ACTIONS],
    /// Engine-suggested semantic waits.
    pub wait_menu: WaitMenu,
}

impl ActionMask {
    /// A mask with nothing castable and nothing becoming castable.
    #[must_use]
    pub fn none() -> Self {
        Self {
            castable_now: 0,
            usable_in: [f32::INFINITY; MAX_ACTIONS],
            wait_menu: WaitMenu::default(),
        }
    }

    /// Is slot `i` castable right now?
    #[must_use]
    pub fn is_castable(&self, slot: SlotId) -> bool {
        usize::from(slot.0) < MAX_ACTIONS && self.castable_now & (1_u64 << slot.0) != 0
    }
}
