//! Search over pre-run decisions.
//!
//! Generic over the point type, so one optimizer serves loadouts, plans, or
//! both together. Budgets are spent adaptively: propose candidates, score
//! them on a few seeds, drop clear losers, and give survivors more seeds.
//! Candidates are scored on shared seeds and compared seed by seed.

use portunus_eval::{Estimate, Metric, PerSeed, SeedSet};

pub trait SearchSpace {
    type Point: Clone;
    fn starting_points(&self) -> Vec<Self::Point>;
    /// Small edits: swap one item, move one talent point, shift one window.
    fn neighbors(&self, point: &Self::Point) -> Vec<Self::Point>;
    fn describe(&self, point: &Self::Point) -> String;
}

/// An upper limit on a metric's mean, e.g. a plan's per-pull death chance.
/// Checked separately, never folded into the objective.
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub metric: Metric,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scores {
    /// Lower is better.
    pub objective: PerSeed,
    pub constraints: Vec<(Constraint, PerSeed)>,
}

/// Usually: build an arm for the point and run it through `portunus-eval`.
pub trait Objective<P> {
    fn evaluate(&self, point: &P, seeds: SeedSet) -> Scores;
}

/// Stays with a candidate across rounds, so a survivor's new seeds add to
/// its earlier scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CandidateId(pub u32);

#[derive(Debug, Clone)]
pub struct Proposal<P> {
    pub id: CandidateId,
    pub point: P,
    pub seeds: SeedSet,
}

pub trait Optimizer<S: SearchSpace> {
    fn propose(&mut self, space: &S) -> Vec<Proposal<S::Point>>;
    fn observe(&mut self, id: CandidateId, scores: Scores);
    /// The best candidate meeting every constraint so far.
    fn best(&self) -> Option<(S::Point, Estimate)>;
    fn finished(&self) -> bool;
}
