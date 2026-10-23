//! What counts as progress.

use portunus_core::SimTime;
use portunus_engine::StateView;
use portunus_scenario::ResolvedRun;
use serde::{Deserialize, Serialize};

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
    /// `run` supplies what the state doesn't carry, such as each spawn's
    /// forces.
    fn measure(&self, state: &dyn StateView, run: &ResolvedRun) -> Progress;
}

/// e.g. negative elapsed time, priority damage dealt, or a blend.
pub trait Reward {
    fn reward(&self, before: &Progress, after: &Progress) -> f64;
}

/// Dense shaped [`Reward`] combining enemy health removed, pull completions,
/// elapsed time cost, and death penalties for finite-horizon search and RL.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DenseReward {
    /// Reward per 1,000 enemy health removed.
    pub per_k_damage: f64,
    /// Bonus per pull cleared.
    pub per_pull_cleared: f64,
    /// Penalty per second elapsed.
    pub per_second: f64,
    /// Penalty per party death.
    pub per_death: f64,
}

impl Default for DenseReward {
    fn default() -> Self {
        Self {
            per_k_damage: 1.0,
            per_pull_cleared: 50.0,
            per_second: 1.0,
            per_death: 100.0,
        }
    }
}

impl Reward for DenseReward {
    fn reward(&self, before: &Progress, after: &Progress) -> f64 {
        let dt = f64::from(after.now.saturating_since(before.now).millis()) / 1000.0;
        let dmg = if after.pulls_cleared == before.pulls_cleared {
            (before.enemy_health - after.enemy_health).max(0.0) / 1000.0
        } else {
            before.enemy_health.max(0.0) / 1000.0
        };
        let pulls = f64::from(after.pulls_cleared.saturating_sub(before.pulls_cleared));
        let deaths = f64::from(after.deaths.saturating_sub(before.deaths));
        self.per_k_damage * dmg + self.per_pull_cleared * pulls
            - self.per_second * dt
            - self.per_death * deaths
    }
}
