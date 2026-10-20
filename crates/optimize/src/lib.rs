//! Search over pre-run decisions.
//!
//! Generic over the point type, so one optimizer serves loadouts, plans, or
//! both together. Budgets are spent adaptively: propose candidates, score
//! them on a few seeds, drop clear losers, and give survivors more seeds.
//! Candidates are scored on shared seeds and compared seed by seed.

use std::collections::{BTreeMap, BTreeSet};

use portunus_eval::{estimate, Estimate, Metric, PerSeed, SeedSet};

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

/// Configuration for [`RacingOptimizer`]: adaptive seed-racing (successive
/// halving + paired-seed CI pruning) combined with local neighborhood
/// hillclimbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RacingConfig {
    /// First seed index used for candidate evaluations.
    pub first_seed: u64,
    /// Number of seeds evaluated in the first racing rung.
    pub initial_seeds: u32,
    /// Factor by which total seeds per survivor grow at each rung.
    pub rung_multiplier: u32,
    /// Number of successive-halving rungs per neighborhood round.
    pub rungs_per_round: usize,
    /// Maximum neighborhood expansion rounds around the best feasible survivor.
    pub max_rounds: usize,
}

impl Default for RacingConfig {
    fn default() -> Self {
        Self {
            first_seed: 0,
            initial_seeds: 4,
            rung_multiplier: 2,
            rungs_per_round: 3,
            max_rounds: 3,
        }
    }
}

#[derive(Debug, Clone)]
struct CandidateRecord<P> {
    id: CandidateId,
    point: P,
    description: String,
    objective_by_seed: BTreeMap<u64, f64>,
    constraints: Vec<(Constraint, BTreeMap<u64, f64>)>,
    next_seed: u64,
    alive_in_cohort: bool,
}

impl<P> CandidateRecord<P> {
    fn objective_values(&self) -> Vec<f64> {
        self.objective_by_seed.values().copied().collect()
    }

    fn objective_estimate(&self) -> Estimate {
        let vals = self.objective_values();
        estimate(&vals)
    }

    fn is_feasible(&self) -> bool {
        if self.objective_by_seed.is_empty() {
            return false;
        }
        self.constraints.iter().all(|(c, map)| {
            let vals: Vec<f64> = map.values().copied().collect();
            !vals.is_empty() && estimate(&vals).mean <= c.max
        })
    }
}

/// Adaptive seed-racing and local neighborhood hillclimbing [`Optimizer`].
///
/// Within each round, a cohort of candidates is raced across increasing seed
/// budgets (`rungs_per_round`):
/// 1. Candidates that violate any [`Constraint`] are dropped.
/// 2. Candidates whose paired seed-by-seed difference against the cohort
///    leader has a 95% confidence interval strictly above zero (`ci95.0 > 0.0`)
///    are dropped early.
/// 3. Successive halving retains the top half of feasible survivors for the
///    next rung with a larger seed budget.
/// 4. At the end of a round, the best overall feasible candidate expands its
///    unseen [`SearchSpace::neighbors`] for the next round.
#[derive(Debug, Clone)]
pub struct RacingOptimizer<P> {
    pub config: RacingConfig,
    candidates: BTreeMap<CandidateId, CandidateRecord<P>>,
    seen_descriptions: BTreeSet<String>,
    cohort: Vec<CandidateId>,
    round: usize,
    rung: usize,
    awaiting_observations: bool,
    done: bool,
    next_id: u32,
}

impl<P: Clone> RacingOptimizer<P> {
    pub fn new(config: RacingConfig) -> Self {
        Self {
            config,
            candidates: BTreeMap::new(),
            seen_descriptions: BTreeSet::new(),
            cohort: Vec::new(),
            round: 0,
            rung: 0,
            awaiting_observations: false,
            done: false,
            next_id: 0,
        }
    }

    /// Returns whether the racing and neighborhood search has terminated.
    pub fn is_finished(&self) -> bool {
        self.done
    }

    /// Returns all evaluated candidates (feasible or not) alongside their
    /// description, feasibility status, and objective [`Estimate`].
    pub fn leaderboard(&self) -> Vec<(CandidateId, String, bool, Estimate)> {
        let mut list: Vec<_> = self
            .candidates
            .values()
            .filter(|c| !c.objective_by_seed.is_empty())
            .map(|c| {
                (
                    c.id,
                    c.description.clone(),
                    c.is_feasible(),
                    c.objective_estimate(),
                )
            })
            .collect();
        list.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.3.mean.total_cmp(&b.3.mean))
                .then_with(|| a.0.cmp(&b.0))
        });
        list
    }

    fn register_point<S: SearchSpace<Point = P>>(
        &mut self,
        space: &S,
        point: P,
    ) -> Option<CandidateId> {
        let desc = space.describe(&point);
        if !self.seen_descriptions.insert(desc.clone()) {
            return None;
        }
        let id = CandidateId(self.next_id);
        self.next_id += 1;
        self.candidates.insert(
            id,
            CandidateRecord {
                id,
                point,
                description: desc,
                objective_by_seed: BTreeMap::new(),
                constraints: Vec::new(),
                next_seed: self.config.first_seed,
                alive_in_cohort: true,
            },
        );
        Some(id)
    }

    fn target_seeds_for_rung(&self, rung: usize) -> u32 {
        let base = self.config.initial_seeds.max(1);
        let mult = self.config.rung_multiplier.max(2);
        let exp = u32::try_from(rung).unwrap_or(0);
        base.saturating_mul(mult.saturating_pow(exp))
    }

    fn prune_cohort(&mut self) {
        for &cid in &self.cohort {
            if let Some(rec) = self.candidates.get_mut(&cid) {
                if !rec.is_feasible() {
                    rec.alive_in_cohort = false;
                }
            }
        }

        let mut alive: Vec<(CandidateId, f64)> = self
            .cohort
            .iter()
            .copied()
            .filter_map(|cid| {
                let rec = self.candidates.get(&cid)?;
                rec.alive_in_cohort
                    .then(|| (cid, rec.objective_estimate().mean))
            })
            .collect();
        if alive.len() <= 1 {
            return;
        }
        alive.sort_by(|a, b| a.1.total_cmp(&b.1));
        let leader_id = alive[0].0;
        let leader_seeds = self.candidates[&leader_id].objective_by_seed.clone();

        // Paired seed-by-seed CI elimination against cohort leader.
        for &(cid, _) in alive.iter().skip(1) {
            if let Some(rec) = self.candidates.get_mut(&cid) {
                let deltas: Vec<f64> = rec
                    .objective_by_seed
                    .iter()
                    .filter_map(|(s, &v)| leader_seeds.get(s).map(|&lv| v - lv))
                    .collect();
                if deltas.len() >= 2 {
                    let est = estimate(&deltas);
                    if est.ci95.0 > 0.0 {
                        rec.alive_in_cohort = false;
                    }
                }
            }
        }

        // Successive halving: keep top half of remaining alive candidates.
        let mut still_alive: Vec<(CandidateId, f64)> = self
            .cohort
            .iter()
            .copied()
            .filter_map(|cid| {
                let rec = self.candidates.get(&cid)?;
                rec.alive_in_cohort
                    .then(|| (cid, rec.objective_estimate().mean))
            })
            .collect();
        still_alive.sort_by(|a, b| a.1.total_cmp(&b.1));
        let keep = still_alive.len().div_ceil(2).max(1);
        for &(cid, _) in still_alive.iter().skip(keep) {
            if let Some(rec) = self.candidates.get_mut(&cid) {
                rec.alive_in_cohort = false;
            }
        }
    }

    fn best_feasible_id(&self) -> Option<CandidateId> {
        let max_n = self
            .candidates
            .values()
            .filter(|c| c.is_feasible())
            .map(|c| c.objective_by_seed.len())
            .max()?;
        self.candidates
            .values()
            .filter(|c| c.is_feasible() && c.objective_by_seed.len() == max_n)
            .min_by(|a, b| {
                a.objective_estimate()
                    .mean
                    .total_cmp(&b.objective_estimate().mean)
            })
            .or_else(|| {
                self.candidates
                    .values()
                    .filter(|c| c.is_feasible())
                    .min_by(|a, b| {
                        a.objective_estimate()
                            .mean
                            .total_cmp(&b.objective_estimate().mean)
                    })
            })
            .map(|c| c.id)
    }

    fn proposals_for_current_rung(&mut self) -> Vec<Proposal<P>> {
        let target_total = self.target_seeds_for_rung(self.rung);
        let mut out = Vec::new();
        for &cid in &self.cohort {
            let Some(rec) = self.candidates.get_mut(&cid) else {
                continue;
            };
            if !rec.alive_in_cohort {
                continue;
            }
            let have = u32::try_from(rec.objective_by_seed.len()).unwrap_or(0);
            let need = target_total.saturating_sub(have);
            if need == 0 {
                continue;
            }
            let seeds = SeedSet {
                first: rec.next_seed,
                count: need,
            };
            rec.next_seed += u64::from(need);
            out.push(Proposal {
                id: rec.id,
                point: rec.point.clone(),
                seeds,
            });
        }
        out
    }
}

impl<S: SearchSpace> Optimizer<S> for RacingOptimizer<S::Point> {
    fn propose(&mut self, space: &S) -> Vec<Proposal<S::Point>> {
        if self.done {
            return Vec::new();
        }

        if !self.awaiting_observations {
            // Initialize round 0 cohort from starting_points (plus neighbors if singleton).
            let starts = space.starting_points();
            if starts.is_empty() {
                self.done = true;
                return Vec::new();
            }
            let first_pt = starts[0].clone();
            for pt in starts {
                if let Some(id) = self.register_point(space, pt) {
                    self.cohort.push(id);
                }
            }
            if self.cohort.len() == 1 {
                for nb in space.neighbors(&first_pt) {
                    if let Some(id) = self.register_point(space, nb) {
                        self.cohort.push(id);
                    }
                }
            }
            self.round = 0;
            self.rung = 0;
            self.awaiting_observations = true;
            return self.proposals_for_current_rung();
        }

        // Advance from completed rung.
        self.prune_cohort();
        let alive_count = self
            .cohort
            .iter()
            .filter(|&&cid| self.candidates.get(&cid).is_some_and(|c| c.alive_in_cohort))
            .count();

        if self.rung + 1 < self.config.rungs_per_round.max(1) && alive_count >= 1 {
            self.rung += 1;
            let props = self.proposals_for_current_rung();
            if !props.is_empty() {
                return props;
            }
        }

        // Round finished; try expanding neighborhood around best feasible candidate.
        while self.round + 1 < self.config.max_rounds {
            self.round += 1;
            self.rung = 0;
            let Some(best_id) = self.best_feasible_id() else {
                self.done = true;
                return Vec::new();
            };
            let best_point = self.candidates[&best_id].point.clone();
            let mut new_cohort = vec![best_id];
            if let Some(rec) = self.candidates.get_mut(&best_id) {
                rec.alive_in_cohort = true;
            }
            for nb in space.neighbors(&best_point) {
                if let Some(id) = self.register_point(space, nb) {
                    new_cohort.push(id);
                }
            }
            if new_cohort.len() <= 1 {
                // Local optimum reached: no unseen neighbors.
                self.done = true;
                return Vec::new();
            }
            self.cohort = new_cohort;
            let props = self.proposals_for_current_rung();
            if !props.is_empty() {
                return props;
            }
        }

        self.done = true;
        Vec::new()
    }

    fn observe(&mut self, id: CandidateId, scores: Scores) {
        let Some(rec) = self.candidates.get_mut(&id) else {
            return;
        };
        for (seed, val) in scores.objective.seeds.iter().zip(scores.objective.values) {
            if let Some(v) = val {
                rec.objective_by_seed.insert(seed.0, v);
            }
        }
        for (constraint, per_seed) in scores.constraints {
            let entry = if let Some((_, map)) =
                rec.constraints.iter_mut().find(|(c, _)| *c == constraint)
            {
                map
            } else {
                rec.constraints.push((constraint, BTreeMap::new()));
                &mut rec.constraints.last_mut().expect("just pushed").1
            };
            for (seed, val) in per_seed.seeds.iter().zip(per_seed.values) {
                if let Some(v) = val {
                    entry.insert(seed.0, v);
                }
            }
        }
    }

    fn best(&self) -> Option<(S::Point, Estimate)> {
        let id = self.best_feasible_id()?;
        let rec = self.candidates.get(&id)?;
        Some((rec.point.clone(), rec.objective_estimate()))
    }

    fn finished(&self) -> bool {
        self.done
    }
}

/// Runs an [`Optimizer`] loop over `space` and `objective` until
/// [`Optimizer::finished`], returning the best feasible point and its
/// [`Estimate`].
pub fn run_optimization<S, O>(
    optimizer: &mut dyn Optimizer<S>,
    space: &S,
    objective: &O,
) -> Option<(S::Point, Estimate)>
where
    S: SearchSpace,
    O: Objective<S::Point>,
{
    while !optimizer.finished() {
        let proposals = optimizer.propose(space);
        if proposals.is_empty() {
            break;
        }
        for prop in proposals {
            let scores = objective.evaluate(&prop.point, prop.seeds);
            optimizer.observe(prop.id, scores);
        }
    }
    optimizer.best()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct QuadraticSpace;

    impl SearchSpace for QuadraticSpace {
        type Point = i32;

        fn starting_points(&self) -> Vec<Self::Point> {
            vec![0]
        }

        fn neighbors(&self, &p: &Self::Point) -> Vec<Self::Point> {
            vec![
                (p - 2).clamp(-10, 10),
                (p - 1).clamp(-10, 10),
                (p + 1).clamp(-10, 10),
                (p + 2).clamp(-10, 10),
            ]
        }

        fn describe(&self, p: &Self::Point) -> String {
            format!("x={p}")
        }
    }

    struct QuadraticObjective;

    impl Objective<i32> for QuadraticObjective {
        fn evaluate(&self, &point: &i32, seeds: SeedSet) -> Scores {
            // Minimum at x = 4, but x > 5 violates the Deaths constraint.
            let base = f64::from((point - 4).pow(2)) * 10.0;
            let values = seeds
                .iter()
                .map(|s| Some(base + (s.0 % 3) as f64 * 0.1))
                .collect();
            let deaths = seeds
                .iter()
                .map(|_| Some(if point > 5 { 1.0 } else { 0.0 }))
                .collect();
            Scores {
                objective: PerSeed { seeds, values },
                constraints: vec![(
                    Constraint {
                        metric: Metric::Deaths,
                        max: 0.0,
                    },
                    PerSeed {
                        seeds,
                        values: deaths,
                    },
                )],
            }
        }
    }

    #[test]
    fn racing_optimizer_hillclimbs_to_feasible_optimum_and_prunes_losers() {
        let mut opt = RacingOptimizer::new(RacingConfig {
            first_seed: 0,
            initial_seeds: 3,
            rung_multiplier: 2,
            rungs_per_round: 2,
            max_rounds: 4,
        });
        let (best_pt, est) =
            run_optimization(&mut opt, &QuadraticSpace, &QuadraticObjective).expect("found best");
        assert_eq!(best_pt, 4);
        assert!(est.mean < 1.0, "est={est:?}");
        assert!(opt.is_finished());
        let board = opt.leaderboard();
        assert!(!board.is_empty());
        assert_eq!(board[0].1, "x=4");
        assert!(board[0].2);
    }
}
