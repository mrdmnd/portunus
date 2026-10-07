//! Playing whole episodes: arms for experiments, and ability breakdowns.

use std::collections::BTreeMap;
use std::sync::Arc;

use portunus_core::{ActorId, PetId, Seed, SpellId};
use portunus_engine::state::ActorKind;
use portunus_engine::trace::{CastEndReason, TraceEvent};
use portunus_engine::{Outcome, StateView};
use portunus_env::{Env, EnvError, Turn};
use portunus_eval::{Arm, ArmError, SeedSet};
use portunus_gamedata::GameData;
use portunus_policy::Policy;

use crate::env::SimEnv;
use crate::observe::{ScriptObserver, SeatObs};

/// One configuration, played by fixed policies (index = seat).
#[derive(Clone)]
pub struct SimArm {
    pub name: String,
    pub env: SimEnv<ScriptObserver>,
    pub policies: Vec<Arc<dyn Policy<SeatObs>>>,
}

impl SimArm {
    /// Play `seed` to the end in `env`.
    fn play(&self, env: &mut SimEnv<ScriptObserver>, seed: Seed) -> Result<Outcome, ArmError> {
        let fail = |e: EnvError| ArmError::Rollout {
            seed,
            message: e.to_string(),
        };
        let mut turn = env.reset(seed).map_err(fail)?;
        loop {
            match turn {
                Turn::Done(outcome) => return Ok(outcome),
                Turn::Decide(d) => {
                    let Some(policy) = self.policies.get(usize::from(d.seat.0)) else {
                        return Err(ArmError::Rollout {
                            seed,
                            message: format!("no policy for seat {}", d.seat.0),
                        });
                    };
                    let choice = policy.act(&d);
                    turn = env.step(choice).map_err(fail)?.next;
                }
            }
        }
    }
}

impl Arm for SimArm {
    fn name(&self) -> &str {
        &self.name
    }

    fn rollout(&self, seed: Seed) -> Result<Outcome, ArmError> {
        self.play(&mut self.env.clone(), seed)
    }
}

/// Damage and casts by spell and by who cast it (a seat or a type of pet),
/// averaged over seeds.
#[derive(Debug, Clone, PartialEq)]
pub struct Breakdown {
    pub runs: u32,
    /// Mean time in combat per run.
    pub combat_secs: f64,
    pub rows: Vec<AbilityRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbilityRow {
    /// The seats' pets of this type cast it; `None` for the seats' own.
    pub pet: Option<PetId>,
    /// `None` for damage no spell was credited with.
    pub spell: Option<SpellId>,
    /// The spell's name, after the pet's for pets' rows.
    pub name: String,
    pub casts: f64,
    pub hits: f64,
    pub crit_rate: f64,
    pub damage: f64,
    pub dps: f64,
    /// Fraction of all player damage.
    pub share: f64,
}

#[derive(Default)]
struct Tally {
    casts: u64,
    hits: u64,
    crits: u64,
    damage: u64,
}

/// Play every seed and total player and pet damage by spell, without
/// overkill (as in `SeatOutcome::damage_done`). The arm's env must record
/// traces.
pub fn breakdown(arm: &SimArm, data: &GameData, seeds: SeedSet) -> Result<Breakdown, ArmError> {
    let mut tallies: BTreeMap<(Option<PetId>, Option<SpellId>), Tally> = BTreeMap::new();
    let mut combat_ms = 0.0;
    for seed in seeds.iter() {
        let mut env = arm.env.clone();
        let outcome = arm.play(&mut env, seed)?;
        combat_ms += outcome
            .pulls
            .iter()
            .map(|p| f64::from(p.cleared.unwrap_or(outcome.end_time).millis() - p.started.millis()))
            .sum::<f64>();
        let trace = env.drain_trace();
        let Some(state) = env.state() else { continue };
        let players: Vec<_> = state.seats().to_vec();
        let credit = |actor: ActorId| match state.actor(actor).map(|a| a.kind) {
            Some(ActorKind::Pet { pet, .. }) => Some(Some(pet)),
            _ => players.contains(&actor).then_some(None),
        };
        let mut health: BTreeMap<ActorId, u64> = BTreeMap::new();
        for r in trace {
            match r.event {
                TraceEvent::Damage(d) => {
                    let left = health
                        .entry(d.target)
                        .or_insert_with(|| state.actor(d.target).map_or(0, |a| a.max_health));
                    let landed = d.amount.min(*left);
                    *left -= landed;
                    if let Some(pet) = credit(d.source) {
                        let t = tallies.entry((pet, d.spell)).or_default();
                        t.hits += 1;
                        t.crits += u64::from(d.crit);
                        t.damage += landed;
                    }
                }
                TraceEvent::Heal(h) => {
                    if let Some(left) = health.get_mut(&h.target) {
                        let max = state.actor(h.target).map_or(0, |a| a.max_health);
                        *left = left.saturating_add(h.amount).min(max);
                    }
                }
                TraceEvent::CastEnd {
                    actor,
                    spell,
                    reason: CastEndReason::Completed,
                } => {
                    if let Some(pet) = credit(actor) {
                        tallies.entry((pet, Some(spell))).or_default().casts += 1;
                    }
                }
                _ => {}
            }
        }
    }
    let runs = seeds.count.max(1);
    let n = f64::from(runs);
    let combat_secs = combat_ms / 1000.0 / n;
    let total = tallies.values().map(|t| t.damage).sum::<u64>() as f64;
    let mut rows: Vec<AbilityRow> = tallies
        .into_iter()
        .map(|((pet, spell), t)| {
            let damage = t.damage as f64 / n;
            let spell_name = match spell {
                Some(s) => data
                    .spells
                    .get(&s)
                    .map_or_else(|| s.0.to_string(), |d| d.name.clone()),
                None => "(uncredited)".into(),
            };
            AbilityRow {
                pet,
                spell,
                name: match pet {
                    Some(p) => {
                        let pet_name = data
                            .pets
                            .get(&p)
                            .map_or_else(|| p.0.to_string(), |d| d.name.clone());
                        format!("{pet_name}: {spell_name}")
                    }
                    None => spell_name,
                },
                casts: t.casts as f64 / n,
                hits: t.hits as f64 / n,
                crit_rate: if t.hits > 0 {
                    t.crits as f64 / t.hits as f64
                } else {
                    0.0
                },
                damage,
                dps: if combat_secs > 0.0 {
                    damage / combat_secs
                } else {
                    0.0
                },
                share: if total > 0.0 {
                    t.damage as f64 / total
                } else {
                    0.0
                },
            }
        })
        .collect();
    rows.sort_by(|a, b| b.damage.total_cmp(&a.damage));
    Ok(Breakdown {
        runs,
        combat_secs,
        rows,
    })
}
