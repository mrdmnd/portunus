//! Run compilation: validate once, then sample into [`ResolvedRun`]s.
//!
//! Every draw is keyed on a *semantic domain* derived from **names** —
//! `hash(pull, spawn_label, script_id)` — plus a per-domain occurrence
//! counter, never a shared sequential stream (invariant 6). Editing or
//! reordering one pull therefore cannot perturb another pull's rolls:
//! common-random-number pairing survives spec edits.

use sim_types::rng::{draw_u64, encounter_domain, uniform01};
use sim_types::{
    from_millis, ActorId, Bestiary, EnemyDef, EngageSpec, HpSpec, IntakeTarget, IntakeTargetSpec,
    JitteredMs, Plan, PlanConstraints, PlannedWindow, PullPlan, ResolvedCombat, ResolvedRun,
    ResolvedSpawn, RunSegment, ScriptEffect, ScriptedEventKind, ScriptedOccurrence, Seed, SimTime,
    TimelinePriors, TriggerExpr, WindowAnchor, PARTY_SIZE,
};
use thiserror::Error;

use crate::run::{AnchorSpec, EngageWave, PullSpec, RunSpec, WhenSpec};
use crate::ScenarioSampler;

/// Validation failure: every problem found, not just the first.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("invalid run spec:\n  {}", .0.join("\n  "))]
    Invalid(Vec<String>),
}

/// Semantic sampling purposes (part of the RNG domain key; renumbering these
/// is a determinism-breaking change).
mod purpose {
    pub const TRAVEL: u16 = 1;
    pub const HP: u16 = 2;
    pub const SCRIPT: u16 = 3;
    pub const TARGET: u16 = 4;
}

/// FNV-1a over name parts: the stable, order-independent domain index.
fn name_domain(parts: &[&str]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for part in parts {
        for b in part.bytes() {
            h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
        }
        h = (h ^ 0x2f).wrapping_mul(0x0100_0193); // separator
    }
    h
}

/// Uniform draw from `base ± jitter` ms (inclusive bounds, integer end to end).
fn sample_jittered(j: JitteredMs, seed: Seed, p: u16, domain: u32, occurrence: u64) -> u64 {
    if j.jitter == 0 {
        return j.base;
    }
    let x = draw_u64(seed, encounter_domain(p, domain), occurrence);
    j.base.saturating_sub(j.jitter) + x % (2 * j.jitter + 1)
}

fn sample_hp(hp: HpSpec, seed: Seed, domain: u32) -> f64 {
    if hp.jitter_frac == 0.0 {
        return hp.mean;
    }
    let u = uniform01(draw_u64(seed, encounter_domain(purpose::HP, domain), 0));
    hp.mean * (1.0 + hp.jitter_frac * (2.0 * u - 1.0))
}

/// One expanded spawn line: `(label, enemy, wave index)`.
type SpawnLine<'s> = (String, &'s str, usize);

/// Expand a pull's waves into labeled spawn lines (`grunt#1`, `grunt#2`, ...;
/// counters continue across waves within the pull).
fn expand_spawns(pull: &PullSpec) -> Vec<SpawnLine<'_>> {
    let mut counters: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
    let mut out = Vec::new();
    for (wi, wave) in pull.engage.iter().enumerate() {
        for (enemy, n) in &wave.mobs {
            for _ in 0..*n {
                let c = counters.entry(enemy.as_str()).or_insert(0);
                *c += 1;
                out.push((format!("{enemy}#{c}"), enemy.as_str(), wi));
            }
        }
    }
    out
}

/// Parse a script-event anchor: `spawn_label/script_id@occurrence`.
fn parse_event_anchor(s: &str) -> Option<(&str, &str, u32)> {
    let (spawn, rest) = s.split_once('/')?;
    let (script, occ) = rest.split_once('@')?;
    Some((spawn, script, occ.parse().ok()?))
}

/// A validated `(RunSpec, Bestiary)` pair, ready to sample.
#[derive(Debug, Clone)]
pub struct RunSampler {
    spec: RunSpec,
    bestiary: Bestiary,
}

impl RunSampler {
    /// Validate the spec against the bestiary; every problem is reported.
    pub fn new(spec: RunSpec, bestiary: Bestiary) -> Result<Self, RunError> {
        let mut errs: Vec<String> = Vec::new();
        let mut total_count: u64 = 0;

        for pull in &spec.pulls {
            let ctx = &pull.name;
            if pull.engage.is_empty() {
                errs.push(format!("pull '{ctx}': no engagement waves"));
            }
            let spawns = expand_spawns(pull);
            if spawns.is_empty() {
                errs.push(format!("pull '{ctx}': pulls nothing"));
            }
            for (wi, wave) in pull.engage.iter().enumerate() {
                if wave.mobs.values().sum::<u32>() == 0 {
                    errs.push(format!("pull '{ctx}' wave {wi}: empty mob list"));
                }
                if wave.after_ms.is_some() && wave.when.is_some() {
                    errs.push(format!(
                        "pull '{ctx}' wave {wi}: both after_ms and when set (pick one)"
                    ));
                }
                if let Some(when) = &wave.when {
                    if when.arity() != 1 {
                        errs.push(format!(
                            "pull '{ctx}' wave {wi}: `when` must set exactly one condition"
                        ));
                    }
                    if let Some(dead) = &when.spawn_dead {
                        if !spawns.iter().any(|(l, _, _)| l == dead) {
                            errs.push(format!(
                                "pull '{ctx}' wave {wi}: spawn_dead '{dead}' names no spawn in this pull"
                            ));
                        }
                    }
                }
                for enemy in wave.mobs.keys() {
                    match bestiary.get(enemy) {
                        None => errs.push(format!(
                            "pull '{ctx}' wave {wi}: enemy '{enemy}' not in bestiary"
                        )),
                        Some(def) => {
                            total_count +=
                                u64::from(def.count) * u64::from(wave.mobs[enemy]);
                            for s in &def.script {
                                if let Some(every) = s.every {
                                    if every.base <= every.jitter {
                                        errs.push(format!(
                                            "enemy '{enemy}' script '{}': every.base must exceed \
                                             every.jitter (minimum gap >= 1ms)",
                                            s.id
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for (ci, cd) in pull.cooldowns.iter().enumerate() {
                if usize::from(cd.player) >= PARTY_SIZE {
                    errs.push(format!(
                        "pull '{ctx}' cooldown {ci}: player {} out of party range",
                        cd.player
                    ));
                }
                if let AnchorSpec::Named(name) = &cd.anchor {
                    if name != "start" {
                        match parse_event_anchor(name) {
                            None => errs.push(format!(
                                "pull '{ctx}' cooldown {ci}: anchor '{name}' is neither 'start', \
                                 an offset, nor 'spawn/script@occ'"
                            )),
                            Some((spawn, script, _)) => {
                                match spawns.iter().find(|(l, _, _)| l == spawn) {
                                    None => errs.push(format!(
                                        "pull '{ctx}' cooldown {ci}: anchor spawn '{spawn}' \
                                         names no spawn in this pull"
                                    )),
                                    Some((_, enemy, _)) => {
                                        let known = bestiary
                                            .get(enemy)
                                            .is_some_and(|d| d.script.iter().any(|s| s.id == script));
                                        if !known {
                                            errs.push(format!(
                                                "pull '{ctx}' cooldown {ci}: '{enemy}' has no \
                                                 script '{script}'"
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(req) = spec.count_requirement {
            if total_count < u64::from(req) {
                errs.push(format!(
                    "count requirement not met: pulled {total_count}, need {req}"
                ));
            }
        }

        if errs.is_empty() {
            Ok(Self { spec, bestiary })
        } else {
            Err(RunError::Invalid(errs))
        }
    }

    #[must_use]
    pub fn spec(&self) -> &RunSpec {
        &self.spec
    }

    /// Compile the static commitments (lust, cooldown windows) into the Plan
    /// artifact policies read. Seed-independent.
    #[must_use]
    pub fn plan(&self) -> Plan {
        let pulls = self
            .spec
            .pulls
            .iter()
            .map(|pull| PullPlan {
                pull: pull.name.clone(),
                lust: pull.lust,
                windows: pull
                    .cooldowns
                    .iter()
                    .map(|cd| PlannedWindow {
                        player: cd.player,
                        ability: cd.ability.clone(),
                        anchor: match &cd.anchor {
                            AnchorSpec::Named(n) if n == "start" => WindowAnchor::PullStart,
                            AnchorSpec::Named(n) => {
                                let (spawn, script, occ) =
                                    parse_event_anchor(n).expect("validated");
                                WindowAnchor::ScriptEvent {
                                    spawn: spawn.to_string(),
                                    script: script.to_string(),
                                    occurrence: occ,
                                }
                            }
                            AnchorSpec::Offset { offset_ms } => {
                                WindowAnchor::Offset(from_millis(*offset_ms))
                            }
                        },
                    })
                    .collect(),
            })
            .collect();
        Plan { pulls, constraints: PlanConstraints::default() }
    }

    /// Unroll one enemy's script into engagement-relative occurrences.
    fn unroll_script(
        &self,
        def: &EnemyDef,
        seed: Seed,
        pull_name: &str,
        label: &str,
        horizon: SimTime,
    ) -> Vec<ScriptedOccurrence> {
        let mut out: Vec<ScriptedOccurrence> = Vec::new();
        for line in &def.script {
            let domain = name_domain(&[pull_name, label, &line.id]);
            let mut draw_idx: u64 = 0;
            let mut t = from_millis(sample_jittered(
                line.first,
                seed,
                purpose::SCRIPT,
                domain,
                draw_idx,
            ));
            let mut emitted: u64 = 0;
            while t <= horizon {
                let kind = match line.effect {
                    ScriptEffect::Intake { amount, target } => ScriptedEventKind::Intake {
                        amount,
                        target: match target {
                            IntakeTargetSpec::AllPlayers => IntakeTarget::AllPlayers,
                            IntakeTargetSpec::RandomPlayer => {
                                let x = draw_u64(
                                    seed,
                                    encounter_domain(purpose::TARGET, domain),
                                    emitted,
                                );
                                IntakeTarget::Player(ActorId((x % PARTY_SIZE as u64) as u8))
                            }
                        },
                    },
                };
                out.push(ScriptedOccurrence { offset: t, kind });
                emitted += 1;
                let Some(every) = line.every else { break };
                draw_idx += 1;
                t += from_millis(sample_jittered(
                    every,
                    seed,
                    purpose::SCRIPT,
                    domain,
                    draw_idx,
                ));
            }
        }
        out.sort_by_key(|o| o.offset);
        out
    }

    fn resolve_when(spawns: &[SpawnLine<'_>], when: &WhenSpec) -> TriggerExpr {
        if let Some(f) = when.engaged_hp_frac_below {
            TriggerExpr::EngagedHpFracBelow(f)
        } else if let Some(n) = when.engaged_alive_at_most {
            TriggerExpr::EngagedAliveAtMost(n)
        } else if let Some(ms) = when.time_at_least_ms {
            TriggerExpr::TimeAtLeast(from_millis(ms))
        } else if let Some(label) = &when.spawn_dead {
            let idx = spawns
                .iter()
                .position(|(l, _, _)| l == label)
                .expect("validated");
            TriggerExpr::SpawnDead(idx as u32)
        } else {
            unreachable!("validated: exactly one condition set")
        }
    }

    fn sample_pull(&self, pull: &PullSpec, seed: Seed) -> ResolvedCombat {
        let timeout = from_millis(pull.timeout_ms);
        let spawns = expand_spawns(pull);
        let resolved = spawns
            .iter()
            .map(|(label, enemy, wi)| {
                let def = self.bestiary.get(enemy).expect("validated");
                let wave: &EngageWave = &pull.engage[*wi];
                let engage = if let Some(ms) = wave.after_ms {
                    EngageSpec::After(from_millis(ms))
                } else if let Some(when) = &wave.when {
                    EngageSpec::When(Self::resolve_when(&spawns, when))
                } else {
                    EngageSpec::AtStart
                };
                ResolvedSpawn {
                    label: label.clone(),
                    enemy: (*enemy).to_string(),
                    max_health: sample_hp(def.hp, seed, name_domain(&[&pull.name, label])),
                    count: def.count,
                    engage,
                    script: self.unroll_script(def, seed, &pull.name, label, timeout),
                }
            })
            .collect();
        ResolvedCombat { name: pull.name.clone(), spawns: resolved, timeout }
    }
}

impl ScenarioSampler for RunSampler {
    fn sample(&self, seed: Seed) -> ResolvedRun {
        let mut segments: Vec<RunSegment> = Vec::new();
        for pull in &self.spec.pulls {
            if let Some(travel) = pull.travel_in {
                segments.push(RunSegment::Travel {
                    duration: from_millis(sample_jittered(
                        travel,
                        seed,
                        purpose::TRAVEL,
                        name_domain(&[&pull.name]),
                        0,
                    )),
                });
            }
            segments.push(RunSegment::Combat(self.sample_pull(pull, seed)));
        }
        ResolvedRun { name: self.spec.name.clone(), segments }
    }

    fn priors(&self) -> TimelinePriors {
        let mut event_means: Vec<(String, SimTime)> = Vec::new();
        for pull in &self.spec.pulls {
            for (label, enemy, _) in expand_spawns(pull) {
                let Some(def) = self.bestiary.get(enemy) else { continue };
                for line in &def.script {
                    if let Some(every) = line.every {
                        event_means.push((
                            format!("{}/{label}/{}", pull.name, line.id),
                            from_millis(every.base),
                        ));
                    }
                }
            }
        }
        TimelinePriors { event_means }
    }
}
