//! What a seat is allowed to see.
//!
//! Observers are the only code given privileged state on a policy's
//! behalf. Under [`InfoSet::Realistic`] an observer must restrict itself to
//! what a player could know in game (and what an addon can read from the
//! game API): visible health, cast bars, and auras, plus the timeline
//! priors — never sampled future timings or pending spawns. It must also
//! hide events the seat hasn't perceived yet (`StateView::unperceived`),
//! and present the legality mask as it looked before them.

use portunus_core::Seat;
use portunus_engine::{ActionMask, StateView};
use portunus_gamedata::{EnemyData, GameData};
use portunus_loadout::ActorTemplate;
use portunus_plan::PullPlan;
use portunus_scenario::{ResolvedRun, TimelinePriors};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoSet {
    /// Sees the sampled truth, including future enemy timings. An upper
    /// bound, and a training aid.
    Privileged,
    /// Sees only what a player could know.
    Realistic,
}

pub struct ObsContext<'a> {
    pub state: &'a dyn StateView,
    pub run: &'a ResolvedRun,
    pub data: &'a GameData,
    pub enemies: &'a EnemyData,
    /// Every seat's compiled configuration, index = seat. Talents, gear,
    /// and set pieces are inspectable in game, so realistic observers may
    /// use them too.
    pub party: &'a [ActorTemplate],
    pub priors: &'a TimelinePriors,
    /// The plan for the pull in progress, if any.
    pub plan: Option<&'a PullPlan>,
    pub info: InfoSet,
}

pub trait Observer {
    /// Structured for scripted policies, or a flat feature vector for models.
    type Obs: Clone;
    fn observe(&self, ctx: &ObsContext<'_>, seat: Seat) -> Self::Obs;
    /// The legality mask the seat is shown, derived from the engine's.
    fn legal(&self, ctx: &ObsContext<'_>, seat: Seat, engine_mask: &ActionMask) -> ActionMask;
}
