//! The simulation kernel.
//!
//! The kernel owns time and bookkeeping: the event queue, cast and GCD
//! gates, cooldowns and charges, aura durations, ticks, and carried values,
//! linearly evolving resources, swing timers, projectiles in flight, pets,
//! enemy rule triggers, engagement, deaths, and pull transitions. It knows
//! nothing about any spec; what spells *do* is delegated to [`Mechanics`].
//!
//! # Pets and auto-attacks
//!
//! Pets and guardians are actors owned by a seat, never seats themselves:
//! they get no decision points, cast their autocast priority on the owner's
//! primary target, and can be commanded by the owner's spells. Every actor
//! with a weapon (or a pet with melee) swings at its primary target on its
//! own timer while in combat. Hard casts and channels pause melee swings
//! and resume them when they end.
//!
//! # When seats decide
//!
//! Control is event-driven, not ticked. Each seat is in one
//! [`state::SeatPhase`]. A seat is asked to decide only when:
//!
//! 1. **it becomes free and something is legal** — a cast ends, the GCD
//!    ends, or a cooldown it was locked behind returns. A seat with nothing
//!    legal is never asked; it sleeps until its mask next changes. Right
//!    after starting a GCD-triggering cast, it is asked again at the same
//!    timestamp only if an off-GCD ability is ready ([`WakeReason::OffGcdReady`]);
//! 2. **its own wait finishes** — [`Wait::Until`], [`Wait::GcdEnd`],
//!    [`Wait::ChannelTick`], or a [`Wait::Condition`] becoming true
//!    (anticipated). A committed seat may also ask to be woken at each tick
//!    of its channel ([`choice::CastOpts::tick_wakes`]);
//! 3. **something interrupts it** — procs, enemy engagements, deaths and
//!    casts, forced movement (unanticipated). These preempt any wait, and
//!    also reach committed seats, which may answer with
//!    [`Choice::StopCast`].
//!
//! Free actions ([`Choice::StopCast`], [`Choice::SetTarget`],
//! [`Choice::CancelAura`]) take no time: the seat is asked again at the same
//! timestamp, in a new batch after the current one applies.
//!
//! Each wake is delivered after the seat's [`setup::Latency`] draw:
//! anticipated wakes model the spell queue window, unanticipated ones cost
//! reaction time. Every unanticipated event gets its own independent
//! delay, so a seat can have several wakes in flight; a second event never
//! reuses or resets the first one's delay. Until an event's delay runs out
//! it is listed in [`StateView::unperceived`], so realistic observers can
//! hide it from decisions that happen in the meantime. Wakes for one seat
//! landing on the same timestamp coalesce into one request.
//!
//! Between pulls, seats are idle until the pull's pre-pull window opens
//! ([`WakeReason::PrePull`]); from then on the same rules apply, except that
//! only non-hostile spells are legal until combat starts.
//!
//! # Ordering and snapshot batches
//!
//! Events sharing a timestamp resolve in [`order::EventClass`] order, and
//! decisions come last, so a policy never sees a half-resolved moment.
//! All seats whose wakes land on the same timestamp form one batch:
//! [`Engine::advance`] hands out the batch's requests one at a time in seat
//! order, every request observes the same state, and [`Engine::submit`]
//! buffers answers until the batch is complete, then applies them in seat
//! order. No seat sees another's same-moment choice.
//!
//! Waiting twice on the same trigger with nothing in between is a
//! [`EngineError::Livelock`]; the combat timeout guarantees every wait ends.
//!
//! # Markov state, determinism, and forking
//!
//! The state is complete: what happens next depends only on the current
//! state and the choices made, never on how the state was reached. No
//! component keeps history; see [`state`] for how "remembered" facts are
//! kept as current ones.
//!
//! Given a [`RunSetup`] and the same sequence of choices, a rollout is
//! bit-identical on every platform, checked via [`Engine::trace_hash`].
//! [`Engine`] is `Clone`, and cloning is the fork that tree search relies
//! on, so it must stay cheap.

pub mod choice;
pub mod error;
pub mod kernel;
pub mod mask;
pub mod mechanics;
pub mod order;
pub mod outcome;
pub mod setup;
pub mod state;
pub mod step;
pub mod trace;

use portunus_core::Seat;

pub use choice::{CastOpts, Choice, Condition, MoveGoal, TargetSel, Wait};
pub use error::{EngineError, IllegalChoice, SetupIssue};
pub use kernel::{Kernel, World};
pub use mask::{ActionMask, Readiness};
pub use mechanics::{EngineIo, Mechanics};
pub use order::EventClass;
pub use outcome::Outcome;
pub use setup::{Externals, Latency, RunSetup, SeatSetup};
pub use state::{
    AuraRef, DeckView, DemandView, ListenerRef, MovementView, PendingTimer, ProcView, SeatPhase,
    SegmentView, StateView,
};
pub use step::{DecisionRequest, Step, WakeReason};
pub use trace::TraceRecord;

pub trait Engine: Clone + Sized {
    type Mechanics: Mechanics;
    type State: StateView;

    fn new(setup: RunSetup, mechanics: Self::Mechanics) -> Result<Self, EngineError>;

    /// Run events until a seat must decide or the run ends. While a batch is
    /// open, returns its next unanswered request (the same one until it is
    /// answered).
    fn advance(&mut self) -> Result<Step, EngineError>;

    /// Answer the current request. Buffered until the batch is complete;
    /// legality is checked against the batch's shared snapshot.
    fn submit(&mut self, seat: Seat, choice: Choice) -> Result<(), EngineError>;

    fn legal(&self, seat: Seat) -> ActionMask;

    /// Full, privileged state. Only observers should read this.
    fn state(&self) -> &Self::State;

    fn trace_hash(&self) -> u64;

    /// Recorded events since the last drain (empty unless the setup asked
    /// for tracing).
    fn drain_trace(&mut self) -> Vec<TraceRecord>;
}
