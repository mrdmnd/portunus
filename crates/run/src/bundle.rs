//! Run files: everything one configuration needs, loaded and checked.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use portunus_core::Seat;
use portunus_engine::{Externals, Latency, SeatSetup};
use portunus_env::InfoSet;
use portunus_eval::{ArmError, Experiment, Metric, SeedSet};
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{
    check_enemy_data, check_game_data, read_ron, DataIssue, EnemyDataSource, GameDataSource,
    IngestError, RonFile,
};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler, LoadoutError};
use portunus_mechanics::{KitIssue, Kits, SpecRegistry};
use portunus_plan::Plan;
use portunus_policy::Policy;
use portunus_scenario::{Sampler, ScenarioError, ScenarioSampler, ScenarioSpec, TimelinePriors};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::arm::{breakdown, Breakdown, SimArm};
use crate::env::{NegElapsed, PartyMeter, SimEnv, Source};
use crate::observe::{ScriptObserver, SeatObs};
use crate::scripts;

/// A run file. Paths are relative to the file itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunConfig {
    pub name: String,
    pub game: PathBuf,
    pub enemies: PathBuf,
    pub scenario: PathBuf,
    #[serde(default)]
    pub plan: Option<PathBuf>,
    /// Optional SimulationCraft `.simc` profile to ingest (full 14-slot gear,
    /// weapons, gems, enchants, trinkets, 2pc/4pc set bonuses, consumables,
    /// and `optimal_raid=1` raid buffs + Bloodlust).
    #[serde(default)]
    pub simc_profile: Option<PathBuf>,
    pub seats: Vec<SeatConfig>,
    #[serde(default)]
    pub externals: Externals,
    #[serde(default = "realistic")]
    pub info: InfoSet,
    #[serde(default)]
    pub first_seed: u64,
    #[serde(default = "default_seeds")]
    pub seeds: u32,
    /// Where the held-out seeds start: as many as `seeds`, kept apart
    /// from the ones a policy is tuned on, to confirm a gain afterwards.
    #[serde(default = "default_holdout_first_seed")]
    pub holdout_first_seed: u64,
}

fn realistic() -> InfoSet {
    InfoSet::Realistic
}

fn default_seeds() -> u32 {
    100
}

fn default_holdout_first_seed() -> u64 {
    1_000_000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatConfig {
    pub loadout: PathBuf,
    /// A policy in [`crate::scripts::SCRIPTS`], by name.
    pub policy: String,
    pub latency: Latency,
}

#[derive(Debug, Error)]
pub enum RunError {
    #[error("{}: {source}", path.display())]
    Read { path: PathBuf, source: IngestError },
    #[error("data problems: {0:?}")]
    Data(Vec<DataIssue>),
    #[error(transparent)]
    Scenario(#[from] ScenarioError),
    #[error("seat {seat}: {source}")]
    Loadout { seat: usize, source: LoadoutError },
    #[error("spec kits: {0:?}")]
    Kits(Vec<KitIssue>),
    #[error("seat {seat}: no policy named {name:?}")]
    UnknownPolicy { seat: usize, name: String },
    #[error("a run needs at least one seat")]
    NoSeats,
    #[error("no seat {0}")]
    NoSuchSeat(usize),
}

/// A loaded, validated run file.
#[derive(Clone)]
pub struct Bundle {
    pub config: RunConfig,
    pub data: Arc<GameData>,
    pub enemies: Arc<EnemyData>,
    pub templates: Vec<ActorTemplate>,
    /// Index = seat.
    pub policies: Vec<Arc<dyn Policy<SeatObs>>>,
    pub plan: Arc<Plan>,
    pub sampler: Arc<Sampler>,
    priors: Arc<TimelinePriors>,
    kits: Arc<dyn SpecRegistry>,
}

impl Bundle {
    pub fn load(path: &Path) -> Result<Self, RunError> {
        let config: RunConfig = read_ron(path).map_err(|source| RunError::Read {
            path: path.to_owned(),
            source,
        })?;
        Self::from_config(config, path.parent().unwrap_or(Path::new(".")))
    }

    /// Paths in `config` are relative to `base`.
    pub fn from_config(config: RunConfig, base: &Path) -> Result<Self, RunError> {
        fn load<T>(
            path: PathBuf,
            f: impl FnOnce(&Path) -> Result<T, IngestError>,
        ) -> Result<T, RunError> {
            f(&path).map_err(|source| RunError::Read { path, source })
        }
        if config.seats.is_empty() {
            return Err(RunError::NoSeats);
        }
        let at = |p: &Path| base.join(p);

        let data: GameData = load(at(&config.game), |p| {
            GameDataSource::load(&RonFile::new(p.to_owned()))
        })?;
        let enemies: EnemyData = load(at(&config.enemies), |p| {
            EnemyDataSource::load(&RonFile::new(p.to_owned()))
        })?;
        let mut issues = check_game_data(&data);
        issues.extend(check_enemy_data(&enemies, &data));
        if !issues.is_empty() {
            return Err(RunError::Data(issues));
        }
        let kits: Arc<dyn SpecRegistry> = Arc::new(Kits::standard());
        let kit_issues = kits.validate(&data);
        if !kit_issues.is_empty() {
            return Err(RunError::Kits(kit_issues));
        }

        let enemies = Arc::new(enemies);
        let spec: ScenarioSpec = load(at(&config.scenario), read_ron)?;
        let sampler = Sampler::new(spec, Arc::clone(&enemies))?;
        let plan: Plan = match &config.plan {
            Some(p) => load(at(p), read_ron)?,
            None => Plan::default(),
        };

        let mut templates = Vec::new();
        let mut policies = Vec::new();
        for (seat, s) in config.seats.iter().enumerate() {
            let loadout: Loadout = load(at(&s.loadout), read_ron)?;
            let template = Compiler
                .compile(&data, &loadout)
                .map_err(|source| RunError::Loadout { seat, source })?;
            let policy = scripts::by_name(&s.policy).ok_or_else(|| RunError::UnknownPolicy {
                seat,
                name: s.policy.clone(),
            })?;
            templates.push(template);
            policies.push(policy);
        }

        let mut bundle = Self {
            priors: Arc::new(sampler.priors()),
            sampler: Arc::new(sampler),
            data: Arc::new(data),
            enemies,
            templates,
            policies,
            plan: Arc::new(plan),
            kits,
            config,
        };
        if let Some(rel_prof) = bundle.config.simc_profile.clone() {
            let prof_path = base.join(&rel_prof);
            if let Some(simc_root) = prof_path
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
            {
                let dump_dir = simc_root.join("SpellDataDump");
                if dump_dir.exists() {
                    let _ = bundle.sync_with_dump(&dump_dir);
                }
            }
            bundle.ingest_simc_profile(&prof_path)?;
            let key = portunus_core::EnemyKey("target_dummy".into());
            if let Some(dummy) = Arc::make_mut(&mut bundle.enemies).enemies.get_mut(&key) {
                if dummy.defense.armor <= 0.0 {
                    dummy.defense.armor = bundle.data.curves.armor_constant * 3.0 / 7.0;
                }
            }
            if let Ok(new_sampler) =
                Sampler::new(bundle.sampler.spec().clone(), Arc::clone(&bundle.enemies))
            {
                bundle.priors = Arc::new(new_sampler.priors());
                bundle.sampler = Arc::new(new_sampler);
            }
        }
        Ok(bundle)
    }

    pub fn seeds(&self) -> SeedSet {
        SeedSet {
            first: self.config.first_seed,
            count: self.config.seeds,
        }
    }

    /// The run's held-out seeds: as many as [`Self::seeds`], from
    /// `holdout_first_seed`.
    pub fn holdout_seeds(&self) -> SeedSet {
        SeedSet {
            first: self.config.holdout_first_seed,
            count: self.config.seeds,
        }
    }

    pub fn source(&self, record_trace: bool) -> Source {
        let sampler = Sampler::new(self.sampler.spec().clone(), Arc::clone(&self.enemies))
            .map(Arc::new)
            .unwrap_or_else(|_| Arc::clone(&self.sampler));
        Source {
            data: Arc::clone(&self.data),
            enemies: Arc::clone(&self.enemies),
            sampler,
            seats: self
                .templates
                .iter()
                .zip(&self.config.seats)
                .map(|(t, s)| SeatSetup {
                    template: t.clone(),
                    latency: s.latency.clone(),
                })
                .collect(),
            externals: self.config.externals.clone(),
            plan: Arc::clone(&self.plan),
            priors: Arc::clone(&self.priors),
            record_trace,
        }
    }

    pub fn env(&self, record_trace: bool) -> SimEnv<ScriptObserver> {
        self.env_with(ScriptObserver, Arc::new(NegElapsed), record_trace)
    }

    pub fn env_with<O: portunus_env::Observer>(
        &self,
        observer: O,
        reward: Arc<dyn portunus_env::Reward + Send + Sync>,
        record_trace: bool,
    ) -> SimEnv<O> {
        SimEnv::new(
            Arc::new(self.source(record_trace)),
            Arc::clone(&self.kits),
            observer,
            Arc::new(PartyMeter),
            reward,
            self.config.info,
        )
    }

    pub fn arm(&self, record_trace: bool) -> SimArm {
        SimArm {
            name: self.config.name.clone(),
            env: self.env(record_trace),
            policies: self.policies.clone(),
        }
    }

    /// [`Self::arm`] with `seat` playing the named policy instead, and the
    /// arm named after it.
    pub fn arm_with(
        &self,
        record_trace: bool,
        seat: usize,
        policy: &str,
    ) -> Result<SimArm, RunError> {
        let mut arm = self.arm(record_trace);
        let resolved = if let Some(p) = scripts::by_name(policy) {
            Some(p)
        } else {
            self.load_burn_policy(seat, policy)
        };
        let slot = arm
            .policies
            .get_mut(seat)
            .ok_or(RunError::NoSuchSeat(seat))?;
        *slot = resolved.ok_or_else(|| RunError::UnknownPolicy {
            seat,
            name: policy.to_owned(),
        })?;
        policy.clone_into(&mut arm.name);
        Ok(arm)
    }

    fn load_burn_policy(
        &self,
        seat: usize,
        policy: &str,
    ) -> Option<Arc<dyn portunus_policy::Policy<crate::observe::SeatObs>>> {
        use portunus_env::{ActionSpace, SpecActionSpace};
        use portunus_learn::{DirStore, ModelStore};
        use portunus_policy::{Featurizer, ModelPolicy};

        let dir = Path::new(policy);
        if !dir.is_dir() {
            return None;
        }
        let store = DirStore::new(dir).ok()?;
        let model_name = format!("{}_burn_rl", self.config.name);
        let version = store.latest(&model_name)?;
        let model = store.load(&version).ok()?;
        let spec_id = self.templates.get(seat)?.spec;
        let space: Arc<dyn ActionSpace> = Arc::new(SpecActionSpace::for_spec(&self.data, spec_id)?);
        let feat: Arc<dyn Featurizer<crate::observe::SeatObs>> = Arc::new(
            crate::observe::SeatObsFeaturizer::for_spec(&self.data, spec_id)?,
        );
        Some(Arc::new(ModelPolicy {
            space,
            featurizer: feat,
            model,
            temperature: 0.0,
        }))
    }

    /// Per-seat damage and DPS, plus party-wide time, completion, deaths,
    /// damage taken, and failed movement demands.
    pub fn experiment(&self, seeds: SeedSet) -> Experiment {
        Experiment {
            arms: vec![Arc::new(self.arm(false))],
            baseline: 0,
            seeds,
            metrics: self.metrics(),
        }
    }

    /// What [`Self::experiment`] measures.
    pub fn metrics(&self) -> Vec<Metric> {
        let mut metrics = Vec::new();
        for s in 0..self.templates.len() {
            let seat = Seat(u8::try_from(s).unwrap_or(u8::MAX));
            metrics.push(Metric::SeatDps(seat));
            metrics.push(Metric::SeatDamage(seat));
        }
        metrics.extend([
            Metric::TotalTime,
            Metric::CompletionRate,
            Metric::Deaths,
            Metric::DamageTaken,
            Metric::DemandsFailed,
        ]);
        metrics
    }

    pub fn breakdown(&self, seeds: SeedSet) -> Result<Breakdown, ArmError> {
        breakdown(&self.arm(true), &self.data, seeds)
    }
}
