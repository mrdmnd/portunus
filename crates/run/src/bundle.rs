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
    pub seats: Vec<SeatConfig>,
    #[serde(default)]
    pub externals: Externals,
    #[serde(default = "realistic")]
    pub info: InfoSet,
    #[serde(default)]
    pub first_seed: u64,
    #[serde(default = "default_seeds")]
    pub seeds: u32,
}

fn realistic() -> InfoSet {
    InfoSet::Realistic
}

fn default_seeds() -> u32 {
    100
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
}

/// A loaded, validated run file.
pub struct Bundle {
    pub config: RunConfig,
    pub data: Arc<GameData>,
    pub enemies: Arc<EnemyData>,
    pub templates: Vec<ActorTemplate>,
    /// Index = seat.
    pub policies: Vec<Arc<dyn Policy<SeatObs>>>,
    pub plan: Arc<Plan>,
    sampler: Arc<Sampler>,
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

        Ok(Self {
            priors: Arc::new(sampler.priors()),
            sampler: Arc::new(sampler),
            data: Arc::new(data),
            enemies,
            templates,
            policies,
            plan: Arc::new(plan),
            kits,
            config,
        })
    }

    pub fn seeds(&self) -> SeedSet {
        SeedSet {
            first: self.config.first_seed,
            count: self.config.seeds,
        }
    }

    pub fn source(&self, record_trace: bool) -> Source {
        Source {
            data: Arc::clone(&self.data),
            enemies: Arc::clone(&self.enemies),
            sampler: Arc::clone(&self.sampler),
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
        SimEnv::new(
            Arc::new(self.source(record_trace)),
            Arc::clone(&self.kits),
            ScriptObserver,
            Arc::new(PartyMeter),
            Arc::new(NegElapsed),
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

    /// Per-seat damage and DPS, plus party-wide time, completion, deaths,
    /// damage taken, and failed movement demands.
    pub fn experiment(&self, seeds: SeedSet) -> Experiment {
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
        Experiment {
            arms: vec![Arc::new(self.arm(false))],
            baseline: 0,
            seeds,
            metrics,
        }
    }

    pub fn breakdown(&self, seeds: SeedSet) -> Result<Breakdown, ArmError> {
        breakdown(&self.arm(true), &self.data, seeds)
    }
}
