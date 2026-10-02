//! What counts as progress.

use portunus_core::SimTime;
use portunus_engine::StateView;

/// A summary of run progress at one moment.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Progress {
    pub now: SimTime,
    pub pulls_cleared: u32,
    pub forces: u32,
    pub deaths: u32,
    /// Remaining health of everything engaged.
    pub enemy_health: f64,
    /// Remaining health weighted by how much each enemy holds up the pull
    /// (a pull lasts as long as its longest-lived enemy).
    pub priority_health: f64,
}

pub trait ProgressMeter {
    fn measure(&self, state: &dyn StateView) -> Progress;
}

/// e.g. negative elapsed time, priority damage dealt, or a blend.
pub trait Reward {
    fn reward(&self, before: &Progress, after: &Progress) -> f64;
}
