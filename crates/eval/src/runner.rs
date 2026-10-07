use std::num::NonZeroUsize;
use std::thread;

use portunus_core::{Seed, SimTime};
use portunus_engine::Outcome;

use crate::{
    Arm, ArmError, ArmReport, Estimate, Experiment, Metric, PairedDelta, PerSeed, Report, Runner,
    SeedSet,
};

/// Runs every arm on every seed across local threads. Results depend only
/// on the experiment, never on the thread count.
#[derive(Debug, Clone, Copy)]
pub struct LocalRunner {
    pub threads: NonZeroUsize,
}

impl LocalRunner {
    /// One thread per available core.
    pub fn available() -> Self {
        Self {
            threads: thread::available_parallelism().unwrap_or(NonZeroUsize::MIN),
        }
    }

    fn outcomes(&self, arm: &dyn Arm, seeds: SeedSet) -> Vec<Result<Outcome, ArmError>> {
        let seeds: Vec<Seed> = seeds.iter().collect();
        let threads = self.threads.get().min(seeds.len().max(1));
        let mut slots: Vec<Option<Result<Outcome, ArmError>>> =
            seeds.iter().map(|_| None).collect();
        thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|t| {
                    let seeds = &seeds;
                    scope.spawn(move || {
                        seeds
                            .iter()
                            .enumerate()
                            .skip(t)
                            .step_by(threads)
                            .map(|(i, &seed)| (i, arm.rollout(seed)))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for worker in workers {
                let done = worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                for (i, result) in done {
                    slots[i] = Some(result);
                }
            }
        });
        slots.into_iter().flatten().collect()
    }
}

impl Runner for LocalRunner {
    fn run(&self, experiment: &Experiment) -> Report {
        let seeds = experiment.seeds;
        let mut failures = Vec::new();
        let mut per_arm: Vec<Vec<PerSeed>> = Vec::new();
        for arm in &experiment.arms {
            let outcomes = self.outcomes(&**arm, seeds);
            for (seed, outcome) in seeds.iter().zip(&outcomes) {
                if outcome.is_err() {
                    failures.push((arm.name().to_owned(), seed));
                }
            }
            per_arm.push(
                experiment
                    .metrics
                    .iter()
                    .map(|m| PerSeed {
                        seeds,
                        values: outcomes
                            .iter()
                            .map(|o| o.as_ref().ok().and_then(|o| metric_value(m, o)))
                            .collect(),
                    })
                    .collect(),
            );
        }

        let arms = experiment
            .arms
            .iter()
            .zip(&per_arm)
            .map(|(arm, values)| ArmReport {
                name: arm.name().to_owned(),
                metrics: experiment
                    .metrics
                    .iter()
                    .zip(values)
                    .map(|(m, v)| {
                        (
                            m.clone(),
                            estimate(&v.values.iter().flatten().copied().collect::<Vec<_>>()),
                        )
                    })
                    .collect(),
            })
            .collect();

        let mut paired = Vec::new();
        if let Some(base) = per_arm.get(experiment.baseline) {
            for (i, (arm, values)) in experiment.arms.iter().zip(&per_arm).enumerate() {
                if i == experiment.baseline {
                    continue;
                }
                for ((metric, ours), theirs) in experiment.metrics.iter().zip(values).zip(base) {
                    let deltas: Vec<f64> = ours
                        .values
                        .iter()
                        .zip(&theirs.values)
                        .filter_map(|(a, b)| Some((*a)? - (*b)?))
                        .collect();
                    paired.push(PairedDelta {
                        arm: arm.name().to_owned(),
                        metric: metric.clone(),
                        delta: estimate(&deltas),
                    });
                }
            }
        }

        Report {
            arms,
            paired,
            failures,
        }
    }
}

fn secs(t: SimTime) -> f64 {
    f64::from(t.millis()) / 1000.0
}

/// A metric's value for one outcome; `None` if the outcome lacks the seat.
pub fn metric_value(metric: &Metric, o: &Outcome) -> Option<f64> {
    let seat = |s: portunus_core::Seat| o.seats.iter().find(|x| x.seat == s);
    match metric {
        Metric::TotalTime => Some(secs(o.end_time)),
        Metric::CompletionRate => Some(if o.completed { 1.0 } else { 0.0 }),
        Metric::Deaths => Some(o.seats.iter().map(|s| f64::from(s.deaths)).sum()),
        Metric::DemandsFailed => Some(o.seats.iter().map(|s| f64::from(s.demands_failed)).sum()),
        Metric::DamageTaken => Some(o.seats.iter().map(|s| s.damage_taken as f64).sum()),
        Metric::SeatDamage(s) => seat(*s).map(|s| s.damage_done as f64),
        Metric::SeatDps(s) => {
            let combat: f64 = o
                .pulls
                .iter()
                .map(|p| secs(p.cleared.unwrap_or(o.end_time)) - secs(p.started))
                .sum();
            seat(*s).map(|s| {
                if combat > 0.0 {
                    s.damage_done as f64 / combat
                } else {
                    0.0
                }
            })
        }
        Metric::DeathChance(pull) => {
            Some(if o.pulls.iter().any(|p| &p.pull == pull && p.deaths > 0) {
                1.0
            } else {
                0.0
            })
        }
    }
}

/// Mean, standard error, and a normal-approximation 95% interval.
pub fn estimate(values: &[f64]) -> Estimate {
    let n = values.len();
    if n == 0 {
        return Estimate {
            n: 0,
            mean: 0.0,
            std_err: 0.0,
            ci95: (0.0, 0.0),
        };
    }
    let nf = n as f64;
    let mean = values.iter().sum::<f64>() / nf;
    let std_err = if n > 1 {
        let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (nf - 1.0);
        (var / nf).sqrt()
    } else {
        0.0
    };
    Estimate {
        n: u32::try_from(n).unwrap_or(u32::MAX),
        mean,
        std_err,
        ci95: (mean - 1.96 * std_err, mean + 1.96 * std_err),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use portunus_core::{PullName, Seat};
    use portunus_engine::outcome::{PullOutcome, SeatOutcome};

    use super::*;

    /// Damage is the seed plus a fixed bonus; seed 3 fails.
    struct Fake {
        name: &'static str,
        bonus: u64,
    }

    impl Arm for Fake {
        fn name(&self) -> &str {
            self.name
        }
        fn rollout(&self, seed: Seed) -> Result<Outcome, ArmError> {
            if seed.0 == 3 {
                return Err(ArmError::Rollout {
                    seed,
                    message: "boom".into(),
                });
            }
            Ok(Outcome {
                completed: true,
                end_time: SimTime(20_000),
                pulls: vec![PullOutcome {
                    pull: PullName("p".into()),
                    started: SimTime(10_000),
                    cleared: Some(SimTime(20_000)),
                    deaths: 0,
                }],
                seats: vec![SeatOutcome {
                    seat: Seat(0),
                    damage_done: seed.0 * 10 + self.bonus,
                    casts: 1,
                    deaths: 0,
                    damage_taken: 0,
                    healing_done: 0,
                    demands_failed: 0,
                }],
            })
        }
    }

    #[test]
    fn estimate_matches_hand_math() {
        let e = estimate(&[1.0, 2.0, 3.0, 4.0]);
        assert_eq!(e.n, 4);
        assert_eq!(e.mean, 2.5);
        assert!((e.std_err - (5.0f64 / 3.0 / 4.0).sqrt()).abs() < 1e-12);
        assert_eq!(estimate(&[]).n, 0);
    }

    #[test]
    fn pairs_by_seed_and_ignores_thread_count() {
        let experiment = Experiment {
            arms: vec![
                Arc::new(Fake {
                    name: "base",
                    bonus: 0,
                }),
                Arc::new(Fake {
                    name: "better",
                    bonus: 50,
                }),
            ],
            baseline: 0,
            seeds: SeedSet { first: 0, count: 8 },
            metrics: vec![Metric::SeatDps(Seat(0)), Metric::CompletionRate],
        };
        let one = LocalRunner {
            threads: NonZeroUsize::MIN,
        }
        .run(&experiment);
        let many = LocalRunner {
            threads: NonZeroUsize::new(3).unwrap(),
        }
        .run(&experiment);
        assert_eq!(one, many);
        assert_eq!(one.failures.len(), 2);
        let dps = &one.paired[0];
        assert_eq!(dps.delta.n, 7);
        assert_eq!(dps.delta.mean, 5.0);
        assert_eq!(dps.delta.std_err, 0.0);
        assert_eq!(one.arms[0].metrics[0].1.n, 7);
    }
}
