use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use portunus_core::rng::{self, Purpose};
use portunus_core::{EnemyKey, Sample, Seed, SimDuration, SpawnLabel, Trigger};
use portunus_gamedata::enemy::EnemyDef;
use portunus_gamedata::EnemyData;

use crate::resolved::{ResolvedCombat, ResolvedRun, ResolvedSpawn, Segment, SpawnIndex, SpawnSet};
use crate::spec::{PullSpec, ScenarioSpec, WaveSubject};
use crate::{EventPrior, ScenarioError, ScenarioIssue, ScenarioSampler, TimelinePriors};

/// Resolves names to indices once, at construction; sampling only draws.
pub struct Sampler {
    spec: ScenarioSpec,
    enemies: Arc<EnemyData>,
    pulls: Vec<Vec<Spawn>>,
}

struct Spawn {
    label: SpawnLabel,
    enemy: EnemyKey,
    engage: Trigger<SpawnSet>,
}

impl ScenarioSampler for Sampler {
    fn new(spec: ScenarioSpec, enemies: Arc<EnemyData>) -> Result<Self, ScenarioError> {
        let mut issues = Vec::new();
        let mut seen = BTreeSet::new();
        let mut pulls = Vec::with_capacity(spec.pulls.len());
        let mut forces = 0;
        for pull in &spec.pulls {
            if !seen.insert(pull.name.clone()) {
                issues.push(ScenarioIssue::DuplicatePull(pull.name.clone()));
            }
            check_travel(pull, &mut issues);
            let spawns = resolve_pull(pull, &enemies, &mut issues);
            forces += spawns
                .iter()
                .filter_map(|s| enemies.enemies.get(&s.enemy))
                .map(|def| def.forces)
                .sum::<u32>();
            pulls.push(spawns);
        }
        if let Some(required) = spec.forces_required {
            if forces < required {
                issues.push(ScenarioIssue::NotEnoughForces {
                    required,
                    available: forces,
                });
            }
        }
        if issues.is_empty() {
            Ok(Self {
                spec,
                enemies,
                pulls,
            })
        } else {
            Err(ScenarioError::Invalid(issues))
        }
    }

    fn spec(&self) -> &ScenarioSpec {
        &self.spec
    }

    fn sample(&self, seed: Seed) -> ResolvedRun {
        let mut segments = Vec::with_capacity(self.pulls.len() * 2);
        for (pull, spawns) in self.spec.pulls.iter().zip(&self.pulls) {
            let name = pull.name.0.as_str();
            let duration =
                pull.travel_in
                    .sample(seed, rng::domain(Purpose::TravelTime, &[name]), 0);
            segments.push(Segment::Travel {
                duration,
                prepull: pull.prepull.min(duration),
            });
            let spawns = spawns
                .iter()
                .map(|s| {
                    let def = self.enemy(&s.enemy);
                    let domain = rng::domain(Purpose::EnemyHealth, &[name, &s.label.0]);
                    ResolvedSpawn {
                        label: s.label.clone(),
                        enemy: s.enemy.clone(),
                        max_health: def.health.sample(seed, domain, 0),
                        forces: def.forces,
                        engage: s.engage.clone(),
                    }
                })
                .collect();
            segments.push(Segment::Combat(ResolvedCombat {
                pull: pull.name.clone(),
                timeout: pull.timeout,
                spawns,
            }));
        }
        ResolvedRun {
            name: self.spec.name.clone(),
            seed,
            segments,
        }
    }

    fn priors(&self) -> TimelinePriors {
        let mut events = Vec::new();
        for (pull, spawns) in self.spec.pulls.iter().zip(&self.pulls) {
            for spawn in spawns {
                for rule in &self.enemy(&spawn.enemy).rules {
                    events.push(EventPrior {
                        pull: pull.name.clone(),
                        spawn: spawn.label.clone(),
                        event: rule.name.clone(),
                        first_mean: first_mean(&rule.when),
                        interval_mean: rule.repeat.as_ref().map(Sample::mean),
                    });
                }
            }
        }
        TimelinePriors { events }
    }
}

impl Sampler {
    /// Only called for enemies validated in `new`.
    fn enemy(&self, key: &EnemyKey) -> &EnemyDef {
        &self.enemies.enemies[key]
    }
}

fn check_travel(pull: &PullSpec, issues: &mut Vec<ScenarioIssue>) {
    let (lo, hi) = pull.travel_in.bounds();
    if lo > hi {
        issues.push(ScenarioIssue::InvalidTravel(pull.name.clone()));
    } else if pull.prepull > lo {
        issues.push(ScenarioIssue::PrepullExceedsTravel(pull.name.clone()));
    }
}

/// Labels every instance `key#n`, counting per key across the pull's waves,
/// then rewrites each wave's trigger in terms of spawn indices.
fn resolve_pull(
    pull: &PullSpec,
    enemies: &EnemyData,
    issues: &mut Vec<ScenarioIssue>,
) -> Vec<Spawn> {
    let mut counts: BTreeMap<&EnemyKey, u32> = BTreeMap::new();
    let mut by_label: BTreeMap<SpawnLabel, (SpawnIndex, &EnemyKey)> = BTreeMap::new();
    let mut by_wave: Vec<Vec<SpawnIndex>> = Vec::with_capacity(pull.waves.len());
    let mut placed: Vec<(SpawnLabel, &EnemyKey, usize)> = Vec::new();
    for (wave_index, wave) in pull.waves.iter().enumerate() {
        let mut members = Vec::new();
        for (key, count) in &wave.mobs {
            if !enemies.enemies.contains_key(key) {
                issues.push(ScenarioIssue::UnknownEnemy {
                    pull: pull.name.clone(),
                    enemy: key.clone(),
                });
            }
            for _ in 0..*count {
                let n = counts.entry(key).or_default();
                *n += 1;
                let label = SpawnLabel(format!("{}#{}", key.0, n));
                let index = SpawnIndex(placed.len() as u16);
                by_label.insert(label.clone(), (index, key));
                members.push(index);
                placed.push((label, key, wave_index));
            }
        }
        by_wave.push(members);
    }
    if placed.is_empty() {
        issues.push(ScenarioIssue::EmptyPull(pull.name.clone()));
    }

    for (wave_index, wave) in pull.waves.iter().enumerate() {
        check_wave_trigger(pull, wave_index, &wave.engage, &by_label, enemies, issues);
    }

    placed
        .into_iter()
        .map(|(label, key, wave_index)| {
            let engage = pull.waves[wave_index].engage.map(&mut |s| match s {
                WaveSubject::Spawn(l) => by_label
                    .get(l)
                    .map_or(SpawnSet::Many(Vec::new()), |(i, _)| SpawnSet::One(*i)),
                WaveSubject::Wave(w) => {
                    SpawnSet::Many(by_wave.get(*w).cloned().unwrap_or_default())
                }
                WaveSubject::Engaged => SpawnSet::Engaged,
            });
            Spawn {
                label,
                enemy: key.clone(),
                engage,
            }
        })
        .collect()
}

fn check_wave_trigger(
    pull: &PullSpec,
    wave_index: usize,
    engage: &Trigger<WaveSubject>,
    by_label: &BTreeMap<SpawnLabel, (SpawnIndex, &EnemyKey)>,
    enemies: &EnemyData,
    issues: &mut Vec<ScenarioIssue>,
) {
    let mut mentions = 0;
    let mut only_self = true;
    engage.visit(&mut |subject, event| {
        mentions += 1;
        match subject {
            WaveSubject::Spawn(label) => {
                only_self = false;
                match by_label.get(label) {
                    None => issues.push(ScenarioIssue::UnknownSpawn {
                        pull: pull.name.clone(),
                        spawn: label.clone(),
                    }),
                    Some((_, key)) => {
                        if let (Some(event), Some(def)) = (event, enemies.enemies.get(*key)) {
                            if !def.rules.iter().any(|r| &r.name == event) {
                                issues.push(ScenarioIssue::UnknownEvent {
                                    pull: pull.name.clone(),
                                    spawn: label.clone(),
                                    event: event.clone(),
                                });
                            }
                        }
                    }
                }
            }
            WaveSubject::Wave(w) => {
                if *w != wave_index {
                    only_self = false;
                }
                if *w >= pull.waves.len() {
                    issues.push(ScenarioIssue::UnknownWave {
                        pull: pull.name.clone(),
                        wave: *w,
                    });
                }
            }
            WaveSubject::Engaged => only_self = false,
        }
    });
    if matches!(engage, Trigger::Never) || (mentions > 0 && only_self) {
        issues.push(ScenarioIssue::NeverEngages {
            pull: pull.name.clone(),
            wave: wave_index,
        });
    }
}

/// Only plain timers have a known first firing.
fn first_mean<S>(when: &Trigger<S>) -> Option<SimDuration> {
    match when {
        Trigger::Now => Some(SimDuration::ZERO),
        Trigger::Elapsed(d) => Some(*d),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portunus_core::{Dist, EventName, PullName};
    use portunus_gamedata::enemy::{EnemyAction, EnemyDefense, EnemyKind, EnemyRule, EnemyTarget};
    use portunus_gamedata::stats::SchoolMask;
    use portunus_gamedata::GameBuild;

    use crate::spec::WaveSpec;

    fn enemies() -> Arc<EnemyData> {
        let grunt = EnemyDef {
            key: EnemyKey("grunt".into()),
            name: "Grunt".into(),
            kind: EnemyKind::Trash,
            health: Dist::Jitter {
                base: 1000.0,
                jitter: 100.0,
            },
            forces: 3,
            defense: EnemyDefense {
                level_offset: 2,
                armor: 0.0,
            },
            initial_phase: None,
            rules: vec![EnemyRule {
                name: EventName("slam".into()),
                phase: None,
                when: Trigger::Elapsed(SimDuration(4_000)),
                repeat: Some(Dist::Fixed(SimDuration(10_000))),
                action: EnemyAction::Damage {
                    amount: Dist::Fixed(50.0),
                    school: SchoolMask::PHYSICAL,
                    target: EnemyTarget::Tank,
                },
            }],
        };
        Arc::new(EnemyData {
            build: GameBuild {
                version: "test".into(),
                build: 0,
            },
            enemies: BTreeMap::from([(grunt.key.clone(), grunt)]),
        })
    }

    fn pull(name: &str, waves: Vec<WaveSpec>) -> PullSpec {
        PullSpec {
            name: PullName(name.into()),
            travel_in: Dist::Uniform {
                lo: SimDuration(5_000),
                hi: SimDuration(8_000),
            },
            prepull: SimDuration(2_000),
            timeout: SimDuration(300_000),
            waves,
        }
    }

    fn wave(count: u32, engage: Trigger<WaveSubject>) -> WaveSpec {
        WaveSpec {
            mobs: vec![(EnemyKey("grunt".into()), count)],
            engage,
        }
    }

    fn spec(pulls: Vec<PullSpec>) -> ScenarioSpec {
        ScenarioSpec {
            name: "test".into(),
            forces_required: None,
            pulls,
        }
    }

    fn issues(spec: ScenarioSpec) -> Vec<ScenarioIssue> {
        match Sampler::new(spec, enemies()) {
            Ok(_) => Vec::new(),
            Err(ScenarioError::Invalid(issues)) => issues,
        }
    }

    #[test]
    fn labels_count_across_waves_and_triggers_resolve() {
        let s = spec(vec![pull(
            "opener",
            vec![
                wave(2, Trigger::Now),
                wave(
                    1,
                    Trigger::Died(WaveSubject::Spawn(SpawnLabel("grunt#1".into()))),
                ),
            ],
        )]);
        let run = Sampler::new(s, enemies()).unwrap().sample(Seed(1));
        let Segment::Combat(combat) = &run.segments[1] else {
            panic!("expected combat");
        };
        let labels: Vec<_> = combat.spawns.iter().map(|s| s.label.0.as_str()).collect();
        assert_eq!(labels, ["grunt#1", "grunt#2", "grunt#3"]);
        assert_eq!(
            combat.spawns[2].engage,
            Trigger::Died(SpawnSet::One(SpawnIndex(0)))
        );
    }

    #[test]
    fn same_seed_same_run_and_draws_in_bounds() {
        let s = spec(vec![pull("a", vec![wave(3, Trigger::Now)])]);
        let sampler = Sampler::new(s, enemies()).unwrap();
        assert_eq!(sampler.sample(Seed(5)), sampler.sample(Seed(5)));
        assert_ne!(sampler.sample(Seed(5)), sampler.sample(Seed(6)));
        let run = sampler.sample(Seed(5));
        let Segment::Travel { duration, prepull } = run.segments[0] else {
            panic!("expected travel");
        };
        assert!((5_000..=8_000).contains(&duration.millis()));
        assert_eq!(prepull, SimDuration(2_000));
        let Segment::Combat(combat) = &run.segments[1] else {
            panic!("expected combat");
        };
        for spawn in &combat.spawns {
            assert!((900.0..=1100.0).contains(&spawn.max_health));
        }
    }

    #[test]
    fn editing_one_pull_leaves_another_pulls_draws_alone() {
        let a = spec(vec![
            pull("a", vec![wave(1, Trigger::Now)]),
            pull("b", vec![wave(2, Trigger::Now)]),
        ]);
        let b = spec(vec![
            pull("a", vec![wave(4, Trigger::Now)]),
            pull("b", vec![wave(2, Trigger::Now)]),
        ]);
        let run_a = Sampler::new(a, enemies()).unwrap().sample(Seed(11));
        let run_b = Sampler::new(b, enemies()).unwrap().sample(Seed(11));
        assert_eq!(run_a.segments[2..], run_b.segments[2..]);
    }

    #[test]
    fn reports_every_problem() {
        let mut bad = pull(
            "a",
            vec![
                WaveSpec {
                    mobs: vec![(EnemyKey("ghost".into()), 1)],
                    engage: Trigger::Now,
                },
                wave(
                    1,
                    Trigger::Fired {
                        who: WaveSubject::Spawn(SpawnLabel("grunt#1".into())),
                        event: EventName("roar".into()),
                        nth: 1,
                    },
                ),
                wave(1, Trigger::Died(WaveSubject::Wave(7))),
                wave(1, Trigger::Died(WaveSubject::Wave(3))),
            ],
        );
        bad.prepull = SimDuration(6_000);
        let mut s = spec(vec![bad, pull("a", vec![wave(1, Trigger::Never)])]);
        s.forces_required = Some(100);
        let found = issues(s);
        let a = PullName("a".into());
        for expected in [
            ScenarioIssue::UnknownEnemy {
                pull: a.clone(),
                enemy: EnemyKey("ghost".into()),
            },
            ScenarioIssue::UnknownEvent {
                pull: a.clone(),
                spawn: SpawnLabel("grunt#1".into()),
                event: EventName("roar".into()),
            },
            ScenarioIssue::UnknownWave {
                pull: a.clone(),
                wave: 7,
            },
            ScenarioIssue::NeverEngages {
                pull: a.clone(),
                wave: 3,
            },
            ScenarioIssue::NeverEngages {
                pull: a.clone(),
                wave: 0,
            },
            ScenarioIssue::PrepullExceedsTravel(a.clone()),
            ScenarioIssue::DuplicatePull(a.clone()),
            ScenarioIssue::NotEnoughForces {
                required: 100,
                available: 12,
            },
        ] {
            assert!(
                found.contains(&expected),
                "missing {expected:?} in {found:?}"
            );
        }
    }

    #[test]
    fn priors_use_means_not_draws() {
        let s = spec(vec![pull("a", vec![wave(1, Trigger::Now)])]);
        let priors = Sampler::new(s, enemies()).unwrap().priors();
        assert_eq!(
            priors.events,
            vec![EventPrior {
                pull: PullName("a".into()),
                spawn: SpawnLabel("grunt#1".into()),
                event: EventName("slam".into()),
                first_mean: Some(SimDuration(4_000)),
                interval_mean: Some(SimDuration(10_000)),
            }]
        );
    }
}
