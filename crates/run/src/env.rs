//! The kernel and real mechanics behind the [`Env`] boundary.

use std::sync::Arc;

use portunus_core::Seed;
use portunus_engine::state::ActorKind;
use portunus_engine::{
    DecisionRequest, Engine, EngineError, Kernel, SegmentView, StateView, Step, TraceRecord, World,
};
use portunus_env::{
    Decision, DecisionKey, Env, EnvError, Episode, EpisodeSource, InfoSet, ObsContext, Observer,
    Progress, ProgressMeter, Reward, Transition, Turn,
};
use portunus_gamedata::{EnemyData, GameData};
use portunus_loadout::ActorTemplate;
use portunus_mechanics::{PartyMechanics, SpecRegistry};
use portunus_plan::{Plan, PullPlan};
use portunus_scenario::resolved::{ResolvedCombat, Segment};
use portunus_scenario::{ResolvedRun, Sampler, ScenarioSampler, TimelinePriors};

/// Samples a scenario per seed and pairs it with fixed seats and a plan.
#[derive(Clone)]
pub struct Source {
    pub data: Arc<GameData>,
    pub enemies: Arc<EnemyData>,
    pub sampler: Arc<Sampler>,
    pub seats: Vec<portunus_engine::SeatSetup>,
    pub externals: portunus_engine::Externals,
    pub plan: Arc<Plan>,
    pub priors: Arc<TimelinePriors>,
    pub record_trace: bool,
}

impl EpisodeSource for Source {
    fn episode(&self, seed: Seed) -> Episode {
        Episode {
            setup: portunus_engine::RunSetup {
                run: Arc::new(self.sampler.sample(seed)),
                data: Arc::clone(&self.data),
                enemies: Arc::clone(&self.enemies),
                seats: self.seats.clone(),
                externals: self.externals.clone(),
                record_trace: self.record_trace,
            },
            plan: Arc::clone(&self.plan),
            priors: Arc::clone(&self.priors),
        }
    }
}

fn combats(run: &ResolvedRun) -> impl Iterator<Item = &ResolvedCombat> {
    run.segments.iter().filter_map(|s| match s {
        Segment::Combat(c) => Some(c),
        Segment::Travel { .. } => None,
    })
}

/// Progress from the live state. Earlier pulls count as cleared, with all
/// their forces.
#[derive(Debug, Clone, Copy, Default)]
pub struct PartyMeter;

impl ProgressMeter for PartyMeter {
    fn measure(&self, state: &dyn StateView, run: &ResolvedRun) -> Progress {
        let all: Vec<&ResolvedCombat> = combats(run).collect();
        let (current, cleared) = match state.segment() {
            SegmentView::Combat(c) => (Some(c.index), usize::from(c.index)),
            SegmentView::Travel { next_combat, .. } => (None, usize::from(next_combat)),
            SegmentView::Finished => (None, all.len()),
        };
        let mut forces: u32 = all
            .iter()
            .take(cleared)
            .flat_map(|c| &c.spawns)
            .map(|s| s.forces)
            .sum();
        let (mut enemy_health, mut priority_health) = (0.0, 0.0f64);
        for &e in state.enemies() {
            let Some(a) = state.actor(e) else { continue };
            if !a.alive {
                if let (ActorKind::Enemy { combat, spawn }, Some(cur)) = (a.kind, current) {
                    if combat == cur {
                        forces += all
                            .get(usize::from(combat))
                            .and_then(|c| c.spawns.get(usize::from(spawn.0)))
                            .map_or(0, |s| s.forces);
                    }
                }
            } else if a.engaged {
                enemy_health += a.health as f64;
                priority_health = priority_health.max(a.health as f64);
            }
        }
        let deaths = state
            .seats()
            .iter()
            .filter(|&&s| state.actor(s).is_some_and(|a| !a.alive))
            .count();
        Progress {
            now: state.now(),
            pulls_cleared: u32::try_from(cleared).unwrap_or(u32::MAX),
            forces,
            deaths: u32::try_from(deaths).unwrap_or(u32::MAX),
            enemy_health,
            priority_health,
        }
    }
}

/// Minus the seconds elapsed: finishing sooner is better.
#[derive(Debug, Clone, Copy, Default)]
pub struct NegElapsed;

impl Reward for NegElapsed {
    fn reward(&self, before: &Progress, after: &Progress) -> f64 {
        -f64::from(after.now.saturating_since(before.now).millis()) / 1000.0
    }
}

#[derive(Clone)]
struct Live {
    kernel: Kernel<PartyMechanics>,
    episode: Episode,
    party: Arc<[ActorTemplate]>,
    seed: Seed,
    pending: Option<DecisionRequest>,
    progress: Progress,
    /// Decisions handed out so far, per seat.
    decisions: Vec<u64>,
    finished: bool,
}

/// [`Env`] over [`Kernel`] and [`PartyMechanics`]. Cloning forks the live
/// episode.
pub struct SimEnv<O> {
    source: Arc<dyn EpisodeSource>,
    kits: Arc<dyn SpecRegistry>,
    observer: Arc<O>,
    meter: Arc<dyn ProgressMeter + Send + Sync>,
    reward: Arc<dyn Reward + Send + Sync>,
    info: InfoSet,
    live: Option<Live>,
}

impl<O> Clone for SimEnv<O> {
    fn clone(&self) -> Self {
        Self {
            source: Arc::clone(&self.source),
            kits: Arc::clone(&self.kits),
            observer: Arc::clone(&self.observer),
            meter: Arc::clone(&self.meter),
            reward: Arc::clone(&self.reward),
            info: self.info,
            live: self.live.clone(),
        }
    }
}

impl<O: Observer> SimEnv<O> {
    pub fn new(
        source: Arc<dyn EpisodeSource>,
        kits: Arc<dyn SpecRegistry>,
        observer: O,
        meter: Arc<dyn ProgressMeter + Send + Sync>,
        reward: Arc<dyn Reward + Send + Sync>,
        info: InfoSet,
    ) -> Self {
        Self {
            source,
            kits,
            observer: Arc::new(observer),
            meter,
            reward,
            info,
            live: None,
        }
    }

    /// The live episode's full state, if one has started.
    pub fn state(&self) -> Option<&World> {
        self.live.as_ref().map(|l| l.kernel.state())
    }

    /// Recorded events since the last drain (empty unless the source
    /// records traces).
    pub fn drain_trace(&mut self) -> Vec<TraceRecord> {
        self.live
            .as_mut()
            .map(|l| l.kernel.drain_trace())
            .unwrap_or_default()
    }

    fn advance(&mut self) -> Result<Turn<O::Obs>, EnvError> {
        let live = self.live.as_mut().ok_or(EnvError::NotStarted)?;
        match live.kernel.advance()? {
            Step::Done(outcome) => {
                live.finished = true;
                live.pending = None;
                Ok(Turn::Done(outcome))
            }
            Step::Decide(req) => {
                live.pending = Some(req);
                let seat = usize::from(req.seat.0);
                let index = live.decisions.get(seat).copied().unwrap_or(0);
                if let Some(n) = live.decisions.get_mut(seat) {
                    *n += 1;
                }
                let live = &*live;
                let setup = &live.episode.setup;
                let state = live.kernel.state();
                let ctx = ObsContext {
                    state,
                    run: &setup.run,
                    data: &setup.data,
                    enemies: &setup.enemies,
                    party: &live.party,
                    priors: &live.episode.priors,
                    plan: pull_plan(&live.episode.plan, &setup.run, state),
                    info: self.info,
                };
                let engine_mask = live.kernel.legal(req.seat);
                Ok(Turn::Decide(Decision {
                    seat: req.seat,
                    reason: req.reason,
                    obs: self.observer.observe(&ctx, req.seat),
                    legal: self.observer.legal(&ctx, req.seat, &engine_mask),
                    key: DecisionKey {
                        seed: live.seed,
                        seat: req.seat,
                        index,
                    },
                }))
            }
        }
    }
}

fn pull_plan<'a>(plan: &'a Plan, run: &ResolvedRun, state: &dyn StateView) -> Option<&'a PullPlan> {
    let SegmentView::Combat(c) = state.segment() else {
        return None;
    };
    let pull = combats(run).nth(usize::from(c.index))?;
    plan.pulls.get(&pull.pull)
}

impl<O: Observer> Env for SimEnv<O> {
    type Obs = O::Obs;

    fn reset(&mut self, seed: Seed) -> Result<Turn<O::Obs>, EnvError> {
        let episode = self.source.episode(seed);
        let mechanics = PartyMechanics::new(&episode.setup, Arc::clone(&self.kits))
            .map_err(|issues| EngineError::Unsupported(format!("spec kits: {issues:?}")))?;
        let party: Arc<[ActorTemplate]> = episode
            .setup
            .seats
            .iter()
            .map(|s| s.template.clone())
            .collect();
        let seats = episode.setup.seats.len();
        let kernel = Kernel::new(episode.setup.clone(), mechanics)?;
        let progress = self.meter.measure(kernel.state(), &episode.setup.run);
        self.live = Some(Live {
            kernel,
            episode,
            party,
            seed,
            pending: None,
            progress,
            decisions: vec![0; seats],
            finished: false,
        });
        self.advance()
    }

    fn step(&mut self, choice: portunus_engine::Choice) -> Result<Transition<O::Obs>, EnvError> {
        let live = self.live.as_mut().ok_or(EnvError::NotStarted)?;
        if live.finished {
            return Err(EnvError::Finished);
        }
        let req = live.pending.ok_or(EnvError::NotStarted)?;
        live.kernel.submit(req.seat, choice)?;
        let before = live.progress;
        let next = self.advance()?;
        let live = self.live.as_mut().ok_or(EnvError::NotStarted)?;
        let after = self
            .meter
            .measure(live.kernel.state(), &live.episode.setup.run);
        live.progress = after;
        Ok(Transition {
            reward: self.reward.reward(&before, &after),
            elapsed: after.now.saturating_since(before.now),
            next,
        })
    }
}
