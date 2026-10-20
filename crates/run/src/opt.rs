//! Pre-run secondary-stat and loadout optimization over a [`Bundle`] using
//! [`portunus_optimize::RacingOptimizer`].

use portunus_core::Seat;
use portunus_eval::{metric_value, Arm, Estimate, LocalRunner, Metric, PerSeed, SeedSet};
use portunus_gamedata::stats::{DerivedStats, Stat, StatBlock};
use portunus_loadout::derive;
use portunus_optimize::{
    run_optimization, CandidateId, Constraint, Objective, RacingConfig, RacingOptimizer, Scores,
    SearchSpace,
};

use crate::bundle::{Bundle, RunError};

const SECONDARIES: [Stat; 4] = [
    Stat::CritRating,
    Stat::HasteRating,
    Stat::MasteryRating,
    Stat::VersatilityRating,
];

/// A secondary-stat allocation point for a single seat, preserving the total
/// secondary rating budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatPoint {
    pub ratings: [i64; 4],
}

impl StatPoint {
    pub fn from_block(block: &StatBlock) -> Self {
        let get = |s: Stat| block.0.get(&s).copied().unwrap_or(0.0).round() as i64;
        Self {
            ratings: [
                get(Stat::CritRating),
                get(Stat::HasteRating),
                get(Stat::MasteryRating),
                get(Stat::VersatilityRating),
            ],
        }
    }

    pub fn apply(&self, base: &StatBlock) -> StatBlock {
        let mut out = base.clone();
        for (s, &v) in SECONDARIES.iter().zip(&self.ratings) {
            out.0.insert(*s, v as f64);
        }
        out
    }
}

/// [`SearchSpace`] that shifts `step_rating` points between pairs of secondary
/// stats (`CritRating`, `HasteRating`, `MasteryRating`, `VersatilityRating`)
/// while keeping the total rating budget constant.
#[derive(Debug, Clone)]
pub struct StatSearchSpace {
    pub start: StatPoint,
    pub step_rating: i64,
    pub min_rating: i64,
}

impl SearchSpace for StatSearchSpace {
    type Point = StatPoint;

    fn starting_points(&self) -> Vec<Self::Point> {
        vec![self.start.clone()]
    }

    fn neighbors(&self, point: &Self::Point) -> Vec<Self::Point> {
        let mut out = Vec::new();
        for from in 0..4 {
            if point.ratings[from] - self.step_rating < self.min_rating {
                continue;
            }
            for to in 0..4 {
                if from == to {
                    continue;
                }
                let mut next = point.clone();
                next.ratings[from] -= self.step_rating;
                next.ratings[to] += self.step_rating;
                out.push(next);
            }
        }
        out
    }

    fn describe(&self, point: &Self::Point) -> String {
        format!(
            "crit={} haste={} mast={} vers={}",
            point.ratings[0], point.ratings[1], point.ratings[2], point.ratings[3]
        )
    }
}

/// Evaluates [`StatPoint`] candidates on a [`Bundle`] using [`LocalRunner`],
/// minimizing `-SeatDps` subject to zero deaths and zero failed mechanics.
pub struct BundleStatObjective<'a> {
    pub bundle: &'a Bundle,
    pub runner: &'a LocalRunner,
    pub seat: usize,
}

impl Objective<StatPoint> for BundleStatObjective<'_> {
    fn evaluate(&self, point: &StatPoint, seeds: SeedSet) -> Scores {
        let seat_id = Seat(u8::try_from(self.seat).unwrap_or(0));
        let mut cand_bundle = self.bundle.clone();
        let tpl = &mut cand_bundle.templates[self.seat];
        tpl.stats = point.apply(&tpl.stats);
        let spec_def = &cand_bundle.data.specs[&tpl.spec];
        tpl.derived = derive(&cand_bundle.data.curves, spec_def, &tpl.stats);
        let arm = cand_bundle.arm(false);

        let dps_metric = Metric::SeatDps(seat_id);
        let deaths_metric = Metric::Deaths;
        let demands_metric = Metric::DemandsFailed;

        type SeedScoreTriplet = (Option<f64>, Option<f64>, Option<f64>);
        let seed_list: Vec<_> = seeds.iter().collect();
        let threads = self.runner.threads.get().min(seed_list.len().max(1));
        let mut slots: Vec<Option<SeedScoreTriplet>> = (0..seed_list.len()).map(|_| None).collect();

        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|t| {
                    let arm = &arm;
                    let seed_list = &seed_list;
                    let dps_metric = &dps_metric;
                    let deaths_metric = &deaths_metric;
                    let demands_metric = &demands_metric;
                    scope.spawn(move || {
                        seed_list
                            .iter()
                            .enumerate()
                            .skip(t)
                            .step_by(threads)
                            .map(|(i, &seed)| match arm.rollout(seed) {
                                Ok(o) => (
                                    i,
                                    (
                                        metric_value(dps_metric, &o).map(|d| -d),
                                        metric_value(deaths_metric, &o),
                                        metric_value(demands_metric, &o),
                                    ),
                                ),
                                Err(_) => (i, (None, None, None)),
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for worker in workers {
                let done = worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                for (i, res) in done {
                    slots[i] = Some(res);
                }
            }
        });

        let mut obj_vals = Vec::with_capacity(seed_list.len());
        let mut death_vals = Vec::with_capacity(seed_list.len());
        let mut demand_vals = Vec::with_capacity(seed_list.len());
        for (obj, dth, dem) in slots.into_iter().flatten() {
            obj_vals.push(obj);
            death_vals.push(dth);
            demand_vals.push(dem);
        }

        Scores {
            objective: PerSeed {
                seeds,
                values: obj_vals,
            },
            constraints: vec![
                (
                    Constraint {
                        metric: Metric::Deaths,
                        max: 0.0,
                    },
                    PerSeed {
                        seeds,
                        values: death_vals,
                    },
                ),
                (
                    Constraint {
                        metric: Metric::DemandsFailed,
                        max: 0.0,
                    },
                    PerSeed {
                        seeds,
                        values: demand_vals,
                    },
                ),
            ],
        }
    }
}

/// Summary of a [`optimize_stats`] run.
#[derive(Debug, Clone)]
pub struct OptimizeReport {
    pub run_name: String,
    pub seat: Seat,
    pub spec_name: String,
    pub start_point: StatPoint,
    pub start_derived: DerivedStats,
    pub start_dps: Estimate,
    pub best_point: StatPoint,
    pub best_derived: DerivedStats,
    pub best_dps: Estimate,
    pub leaderboard: Vec<(CandidateId, String, bool, Estimate)>,
}

/// Runs [`RacingOptimizer`] over secondary-stat allocations for `seat` on `bundle`.
pub fn optimize_stats(
    bundle: &Bundle,
    runner: &LocalRunner,
    seat: usize,
    config: RacingConfig,
    step_rating: i64,
) -> Result<OptimizeReport, RunError> {
    if seat >= bundle.templates.len() {
        return Err(RunError::NoSuchSeat(seat));
    }
    let seat_id = Seat(u8::try_from(seat).unwrap_or(0));
    let tpl = &bundle.templates[seat];
    let spec_def = &bundle.data.specs[&tpl.spec];
    let start_point = StatPoint::from_block(&tpl.stats);
    let start_derived = tpl.derived;

    let total_budget: i64 = start_point.ratings.iter().sum();
    let effective_step = if step_rating > 0 {
        step_rating.min((total_budget / 2).max(50))
    } else {
        (total_budget / 4).clamp(50, 400)
    };
    let space = StatSearchSpace {
        start: start_point.clone(),
        step_rating: effective_step,
        min_rating: 0,
    };
    let objective = BundleStatObjective {
        bundle,
        runner,
        seat,
    };
    let mut opt = RacingOptimizer::new(config);
    let (best_point, best_neg_dps) = run_optimization(&mut opt, &space, &objective).unwrap_or((
        start_point.clone(),
        Estimate {
            n: 0,
            mean: 0.0,
            std_err: 0.0,
            ci95: (0.0, 0.0),
        },
    ));

    let best_stats = best_point.apply(&tpl.stats);
    let best_derived = derive(&bundle.data.curves, spec_def, &best_stats);
    let best_dps = Estimate {
        n: best_neg_dps.n,
        mean: -best_neg_dps.mean,
        std_err: best_neg_dps.std_err,
        ci95: (-best_neg_dps.ci95.1, -best_neg_dps.ci95.0),
    };

    let board: Vec<_> = opt
        .leaderboard()
        .into_iter()
        .map(|(id, desc, feas, est)| {
            (
                id,
                desc,
                feas,
                Estimate {
                    n: est.n,
                    mean: -est.mean,
                    std_err: est.std_err,
                    ci95: (-est.ci95.1, -est.ci95.0),
                },
            )
        })
        .collect();
    let start_dps = board
        .iter()
        .find(|(id, _, _, _)| id.0 == 0)
        .map(|(_, _, _, e)| *e)
        .unwrap_or(best_dps);

    Ok(OptimizeReport {
        run_name: bundle.config.name.clone(),
        seat: seat_id,
        spec_name: spec_def.name.clone(),
        start_point,
        start_derived,
        start_dps,
        best_point,
        best_derived,
        best_dps,
        leaderboard: board,
    })
}
