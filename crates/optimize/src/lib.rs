//! Search over pre-run decisions.
//!
//! Generic over the point type, so one optimizer serves loadouts, plans, or
//! both together. Budgets are spent adaptively: propose candidates, score
//! them on a few seeds, drop clear losers, and give survivors more seeds.

use portunus_eval::{Estimate, SeedSet};

pub trait SearchSpace {
    type Point: Clone;
    fn starting_points(&self) -> Vec<Self::Point>;
    /// Small edits: swap one item, move one talent point, shift one window.
    fn neighbors(&self, point: &Self::Point) -> Vec<Self::Point>;
    fn describe(&self, point: &Self::Point) -> String;
}

/// Usually: build an arm for the point and run it through `portunus-eval`.
pub trait Objective<P> {
    fn evaluate(&self, point: &P, seeds: SeedSet) -> Estimate;
}

#[derive(Debug, Clone)]
pub struct Proposal<P> {
    pub point: P,
    pub seeds: SeedSet,
}

pub trait Optimizer<P> {
    fn propose(&mut self) -> Vec<Proposal<P>>;
    fn observe(&mut self, point: &P, estimate: Estimate);
    fn best(&self) -> Option<(P, Estimate)>;
    fn finished(&self) -> bool;
}
