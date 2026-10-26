//! The plan: group-level commitments made before the run.
//!
//! A decision variable, not part of the world: it can be searched over like a
//! loadout. Windows are advice, never forced casts; observers expose them to
//! policies and featurizers, which own execution if the assigned player is
//! mid-cast, moving, or dead. Bloodlust is just a window on the lust spell.

use std::collections::BTreeMap;

use portunus_core::{
    EnemyKey, EventName, PullName, Seat, SimDuration, SimOffset, SpawnLabel, SpellId,
};
use portunus_gamedata::EnemyData;
use portunus_loadout::ActorTemplate;
use portunus_scenario::ScenarioSpec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub pulls: BTreeMap<PullName, PullPlan>,
    pub constraints: PlanConstraints,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PullPlan {
    pub windows: Vec<PlannedWindow>,
}

/// Seat X should use spell Y on seat W, at anchor Z plus an offset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedWindow {
    pub seat: Seat,
    pub spell: SpellId,
    /// For ally-targeted spells, e.g. Power Infusion.
    pub target: Option<Seat>,
    pub anchor: Anchor,
    /// Negative is before the anchor, e.g. a pre-pull potion.
    pub offset: SimOffset,
    /// How early or late still counts as on plan.
    pub tolerance: SimDuration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    PullStart,
    SpawnEngaged(SpawnLabel),
    /// The `nth` firing (1-based) of a spawn's named rule, e.g. defensive on
    /// the boss's second slam.
    EnemyEvent {
        spawn: SpawnLabel,
        event: EventName,
        nth: u32,
    },
}

/// Limits reported alongside scores, never folded into them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanConstraints {
    /// Maximum acceptable chance of any death in a pull.
    pub max_death_prob_per_pull: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanIssue {
    UnknownPull(PullName),
    UnknownSeat(Seat),
    /// The seat's loadout doesn't grant this spell.
    SpellNotKnown {
        seat: Seat,
        spell: SpellId,
    },
    UnknownTarget(Seat),
    UnknownSpawn {
        pull: PullName,
        spawn: SpawnLabel,
    },
    UnknownEvent {
        pull: PullName,
        spawn: SpawnLabel,
        event: EventName,
    },
    /// The death-chance limit isn't a probability.
    BadDeathLimit,
}

/// Validation interface for plan-aware loaders.
pub trait PlanValidator {
    /// Every problem, not just the first. `party` is indexed by seat.
    fn validate(
        &self,
        plan: &Plan,
        scenario: &ScenarioSpec,
        enemies: &EnemyData,
        party: &[ActorTemplate],
    ) -> Vec<PlanIssue>;
}

/// The standard [`PlanValidator`] implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Validator;

impl PlanValidator for Validator {
    fn validate(
        &self,
        plan: &Plan,
        scenario: &ScenarioSpec,
        enemies: &EnemyData,
        party: &[ActorTemplate],
    ) -> Vec<PlanIssue> {
        let mut issues = Vec::new();
        if let Some(p) = plan.constraints.max_death_prob_per_pull {
            if !(0.0..=1.0).contains(&p) || !p.is_finite() {
                issues.push(PlanIssue::BadDeathLimit);
            }
        }
        for (pull_name, pull_plan) in &plan.pulls {
            let pull_spec = scenario.pulls.iter().find(|p| &p.name == pull_name);
            if pull_spec.is_none() {
                issues.push(PlanIssue::UnknownPull(pull_name.clone()));
            }
            let mut spawns: BTreeMap<SpawnLabel, &EnemyKey> = BTreeMap::new();
            if let Some(spec) = pull_spec {
                let mut counts: BTreeMap<&EnemyKey, u32> = BTreeMap::new();
                for wave in &spec.waves {
                    for (key, count) in &wave.mobs {
                        for _ in 0..*count {
                            let n = counts.entry(key).or_default();
                            *n += 1;
                            spawns.insert(SpawnLabel(format!("{}#{n}", key.0)), key);
                        }
                    }
                }
            }
            for w in &pull_plan.windows {
                match party.get(usize::from(w.seat.0)) {
                    None => issues.push(PlanIssue::UnknownSeat(w.seat)),
                    Some(t) if !t.abilities.contains(&w.spell) => {
                        issues.push(PlanIssue::SpellNotKnown {
                            seat: w.seat,
                            spell: w.spell,
                        });
                    }
                    Some(_) => {}
                }
                if let Some(target) = w.target {
                    if usize::from(target.0) >= party.len() {
                        issues.push(PlanIssue::UnknownTarget(target));
                    }
                }
                if pull_spec.is_some() {
                    match &w.anchor {
                        Anchor::PullStart => {}
                        Anchor::SpawnEngaged(spawn) => {
                            if !spawns.contains_key(spawn) {
                                issues.push(PlanIssue::UnknownSpawn {
                                    pull: pull_name.clone(),
                                    spawn: spawn.clone(),
                                });
                            }
                        }
                        Anchor::EnemyEvent { spawn, event, nth } => match spawns.get(spawn) {
                            None => issues.push(PlanIssue::UnknownSpawn {
                                pull: pull_name.clone(),
                                spawn: spawn.clone(),
                            }),
                            Some(key) => {
                                let has_rule = *nth > 0
                                    && enemies.enemies.get(*key).is_some_and(|def| {
                                        def.rules.iter().any(|r| &r.name == event)
                                    });
                                if !has_rule {
                                    issues.push(PlanIssue::UnknownEvent {
                                        pull: pull_name.clone(),
                                        spawn: spawn.clone(),
                                        event: event.clone(),
                                    });
                                }
                            }
                        },
                    }
                }
            }
        }
        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use portunus_core::{Dist, SpecId, Trigger};
    use portunus_gamedata::enemy::{EnemyDef, EnemyDefense, EnemyKind, EnemyRule};
    use portunus_gamedata::spec::Role;
    use portunus_gamedata::stats::{DerivedStats, SchoolMask, StatBlock};
    use portunus_gamedata::GameBuild;
    use portunus_loadout::{Loadout, TalentSelection};
    use portunus_scenario::spec::{PullSpec, WaveSpec};
    use std::collections::BTreeSet;

    fn dummy_template(spells: &[SpellId]) -> ActorTemplate {
        ActorTemplate {
            spec: SpecId(262),
            role: Role::Damage,
            melee: false,
            hero_tree: None,
            stats: StatBlock::default(),
            derived: DerivedStats::default(),
            resources: Vec::new(),
            abilities: spells.iter().copied().collect::<BTreeSet<_>>(),
            passive_auras: Vec::new(),
            main_hand: None,
            off_hand: None,
            permanent_pet: None,
            source: Loadout {
                spec: SpecId(262),
                gear: BTreeMap::new(),
                hero_tree: None,
                talents: TalentSelection::default(),
                consumables: Vec::new(),
            },
        }
    }

    #[test]
    fn plan_validator_catches_every_issue_kind() {
        let boss_key = EnemyKey("boss".into());
        let mut enemies = EnemyData {
            build: GameBuild {
                version: "test".into(),
                build: 0,
            },
            enemies: BTreeMap::new(),
        };
        enemies.enemies.insert(
            boss_key.clone(),
            EnemyDef {
                key: boss_key.clone(),
                name: "Boss".into(),
                kind: EnemyKind::Boss,
                health: 1_000_000,
                forces: 0,
                defense: EnemyDefense {
                    level_offset: 3,
                    armor: 0.0,
                },
                initial_phase: None,
                rules: vec![EnemyRule {
                    name: EventName("slam".into()),
                    phase: None,
                    when: Trigger::Elapsed(SimDuration(5000)),
                    repeat: None,
                    action: portunus_gamedata::enemy::EnemyAction::Damage {
                        amount: Dist::Fixed(100.0),
                        school: SchoolMask::PHYSICAL,
                        target: portunus_gamedata::enemy::EnemyTarget::Tank,
                    },
                }],
                cc_immune: true,
            },
        );
        let scenario = ScenarioSpec {
            name: "test".into(),
            forces_required: None,
            pulls: vec![PullSpec {
                name: PullName("pull_1".into()),
                travel_in: Dist::Fixed(SimDuration(5000)),
                prepull: SimDuration(2000),
                timeout: SimDuration(60_000),
                health: Dist::Fixed(1.0),
                waves: vec![WaveSpec {
                    mobs: vec![(boss_key, 1)],
                    engage: Trigger::Now,
                    distance: 20.0,
                    spread: None,
                }],
            }],
        };
        let party = vec![dummy_template(&[SpellId(198067)])];

        let mut plan = Plan {
            pulls: BTreeMap::new(),
            constraints: PlanConstraints {
                max_death_prob_per_pull: Some(1.5),
            },
        };
        plan.pulls.insert(
            PullName("missing_pull".into()),
            PullPlan {
                windows: Vec::new(),
            },
        );
        plan.pulls.insert(
            PullName("pull_1".into()),
            PullPlan {
                windows: vec![
                    PlannedWindow {
                        seat: Seat(2),
                        spell: SpellId(198067),
                        target: Some(Seat(3)),
                        anchor: Anchor::SpawnEngaged(SpawnLabel("ghost#1".into())),
                        offset: SimOffset(0),
                        tolerance: SimDuration(1000),
                    },
                    PlannedWindow {
                        seat: Seat(0),
                        spell: SpellId(999999),
                        target: None,
                        anchor: Anchor::EnemyEvent {
                            spawn: SpawnLabel("boss#1".into()),
                            event: EventName("no_such_event".into()),
                            nth: 1,
                        },
                        offset: SimOffset(0),
                        tolerance: SimDuration(1000),
                    },
                ],
            },
        );

        let issues = Validator.validate(&plan, &scenario, &enemies, &party);
        assert!(issues.contains(&PlanIssue::BadDeathLimit));
        assert!(issues.contains(&PlanIssue::UnknownPull(PullName("missing_pull".into()))));
        assert!(issues.contains(&PlanIssue::UnknownSeat(Seat(2))));
        assert!(issues.contains(&PlanIssue::UnknownTarget(Seat(3))));
        assert!(issues.contains(&PlanIssue::UnknownSpawn {
            pull: PullName("pull_1".into()),
            spawn: SpawnLabel("ghost#1".into()),
        }));
        assert!(issues.contains(&PlanIssue::SpellNotKnown {
            seat: Seat(0),
            spell: SpellId(999999),
        }));
        assert!(issues.contains(&PlanIssue::UnknownEvent {
            pull: PullName("pull_1".into()),
            spawn: SpawnLabel("boss#1".into()),
            event: EventName("no_such_event".into()),
        }));
    }
}
