//! Self-play and training through the [Burn](https://burn.dev) deep learning
//! framework.
//!
//! The simulator side collects [`Trajectory`]s via [`SelfPlayCollector`]; a
//! [`BurnTrainer`] trains a Burn [`PolicyValueNet`] using automatic
//! differentiation and [`burn::optim::AdamWConfig`], and produces checkpoints
//! held by a [`ModelStore`].

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use burn::nn::loss::{MseLoss, Reduction};
use burn::optim::{AdamWConfig, GradientsParams, LearningRate, ModuleOptimizer};
use burn::prelude::*;
use burn::tensor::activation::log_softmax;
use portunus_core::{Seat, Seed};
use portunus_engine::Outcome;
use portunus_env::{ActionSpace, Env, Turn};
use portunus_policy::{
    BurnModel, Featurizer, Mcts, MctsConfig, Model, ModelVersion, Policy, PolicyValueNet,
    PolicyValueNetConfig, SchemaId, SearchPolicy,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub seat: Seat,
    pub features: Vec<f32>,
    pub legal: Vec<bool>,
    /// Search visit shares.
    pub policy_target: Vec<f32>,
    /// Discounted return from this decision.
    pub value_target: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Trajectory {
    pub seed: Seed,
    pub model: ModelVersion,
    /// The encoding every sample in this trajectory uses.
    pub schema: SchemaId,
    pub samples: Vec<Sample>,
    pub outcome: Outcome,
}

/// Plays episodes with a search policy and records what it did.
pub trait Collector {
    fn collect(&mut self, model: Arc<dyn Model>, seeds: &[Seed]) -> Vec<Trajectory>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct TrainStats {
    pub policy_loss: f32,
    pub value_loss: f32,
    pub samples_seen: u64,
}

/// A trained Burn model checkpoint, serialized in Burn's `burnpack` record
/// format alongside model metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub version: ModelVersion,
    pub schema: SchemaId,
    pub weights: Vec<u8>,
}

pub trait Trainer {
    fn schema(&self) -> &SchemaId;
    /// Every sample must come from a trajectory with this trainer's schema.
    fn train(&mut self, batch: &[Sample]) -> TrainStats;
    fn checkpoint(&mut self) -> Checkpoint;
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("unknown model version {0:?}")]
    Unknown(ModelVersion),
    #[error("version {0:?} already stored")]
    Exists(ModelVersion),
    #[error("io: {0}")]
    Io(String),
}

pub trait ModelStore {
    fn latest(&self, name: &str) -> Option<ModelVersion>;
    fn load(&self, version: &ModelVersion) -> Result<Arc<dyn Model>, StoreError>;
    /// Versions are immutable once saved.
    fn save(&mut self, checkpoint: &Checkpoint) -> Result<(), StoreError>;
}

/// Self-play [`Collector`] that runs [`Mcts`] over an [`Env`] for each seed.
pub struct SelfPlayCollector<E: Env> {
    pub env: E,
    pub config: MctsConfig,
    pub space: Arc<dyn ActionSpace>,
    pub featurizer: Arc<dyn Featurizer<E::Obs>>,
    /// Per-step reward discount factor in `(0.0, 1.0]`.
    pub discount: f32,
    /// Safety cap on decisions per episode.
    pub max_steps: usize,
    /// Worker threads used to collect episodes in parallel.
    pub threads: NonZeroUsize,
}

impl<E: Env> SelfPlayCollector<E> {
    pub fn new(
        env: E,
        config: MctsConfig,
        space: Arc<dyn ActionSpace>,
        featurizer: Arc<dyn Featurizer<E::Obs>>,
    ) -> Self {
        Self {
            env,
            config,
            space,
            featurizer,
            discount: 1.0,
            max_steps: 10_000,
            threads: thread::available_parallelism().unwrap_or(NonZeroUsize::MIN),
        }
    }
}

impl<E: Env> SelfPlayCollector<E> {
    fn rollout_mcts_seed(
        &self,
        mut env: E,
        model: Arc<dyn Model>,
        seed: Seed,
    ) -> Option<Trajectory> {
        let mut mcts = Mcts::new(
            self.config,
            Arc::clone(&self.space),
            Arc::clone(&self.featurizer),
            Arc::clone(&model),
        );
        let mut turn = env.reset(seed).ok()?;
        if seed.0 >= 2 && self.max_steps < 500 {
            let skip = ((seed.0 as usize).saturating_sub(1) % 5) * (self.max_steps * 2 / 3);
            if skip > 0 {
                let ff_policy = portunus_policy::ModelPolicy {
                    space: Arc::clone(&self.space),
                    featurizer: Arc::clone(&self.featurizer),
                    model: Arc::clone(&model),
                    temperature: 0.0,
                };
                let mut finished_early = false;
                for _ in 0..skip {
                    match &turn {
                        Turn::Done(_) => {
                            finished_early = true;
                            break;
                        }
                        Turn::Decide(d) => {
                            let ch = ff_policy.act(d);
                            if let Ok(tr) = env.step(ch) {
                                turn = tr.next;
                            } else {
                                finished_early = true;
                                break;
                            }
                        }
                    }
                }
                if finished_early || matches!(turn, Turn::Done(_)) {
                    turn = env.reset(seed).ok()?;
                }
            }
        }
        struct PendingStep {
            seat: Seat,
            features: Vec<f32>,
            legal: Vec<bool>,
            policy_target: Vec<f32>,
            reward: f64,
            record_sample: bool,
        }
        let mut pending: Vec<PendingStep> = Vec::new();
        let full_cfg = self.config;
        let fast_cfg = MctsConfig {
            simulations: (self.config.simulations / 4).max(4),
            horizon: portunus_core::SimDuration((self.config.horizon.millis() / 2).max(2_500)),
            ..self.config
        };
        let mut step_idx = 0usize;
        let outcome = loop {
            match turn {
                Turn::Done(o) => break o,
                Turn::Decide(d) => {
                    if pending.len() >= self.max_steps {
                        break Outcome {
                            completed: false,
                            end_time: portunus_core::SimTime::ZERO,
                            seats: Vec::new(),
                            pulls: Vec::new(),
                        };
                    }
                    let features = self.featurizer.featurize(&d.obs, &d.legal);
                    let legal = self.space.legal(&d.legal);
                    let legal_count = legal.iter().filter(|&&b| b).count();
                    // KataGo Playout Cap Oscillation: run full Gumbel MCTS on high-leverage
                    // steps and fast playout cap on low-branching or alternating filler steps.
                    let is_full_search = if full_cfg.simulations < 16 {
                        true
                    } else if legal_count <= 2 {
                        false
                    } else {
                        (step_idx + seed.0 as usize).is_multiple_of(2)
                    };
                    mcts.config = if is_full_search { full_cfg } else { fast_cfg };
                    let sr = mcts.search(&env, &d);
                    let Ok(tr) = env.step(sr.choice) else {
                        return None;
                    };
                    pending.push(PendingStep {
                        seat: d.seat,
                        features,
                        legal,
                        policy_target: sr.visits,
                        reward: tr.reward,
                        record_sample: is_full_search,
                    });
                    step_idx += 1;
                    turn = tr.next;
                }
            }
        };
        if pending.is_empty() {
            return None;
        }

        let gamma = f64::from(self.discount.clamp(0.0, 1.0));
        let mut running_return = 0.0f64;
        let mut samples: Vec<Sample> = Vec::with_capacity(pending.len());
        let any_recorded = pending.iter().any(|p| p.record_sample);
        for step in pending.into_iter().rev() {
            running_return = step.reward + gamma * running_return;
            if step.record_sample || !any_recorded {
                samples.push(Sample {
                    seat: step.seat,
                    features: step.features,
                    legal: step.legal,
                    policy_target: step.policy_target,
                    value_target: running_return as f32,
                });
            }
        }
        samples.reverse();

        Some(Trajectory {
            seed,
            model: model.version(),
            schema: self.featurizer.schema().clone(),
            samples,
            outcome,
        })
    }

    /// Collects expert demonstration trajectories from an arbitrary [`Policy`]
    /// (such as a scripted SimC priority list) for warm-start imitation
    /// pre-training before MCTS self-play.
    pub fn collect_with_policy(
        &self,
        policy: &dyn Policy<E::Obs>,
        version: ModelVersion,
        seeds: &[Seed],
    ) -> Vec<Trajectory> {
        let mut out = Vec::with_capacity(seeds.len());
        let space_size = self.space.size();
        let gamma = f64::from(self.discount.clamp(0.0, 1.0));

        for &seed in seeds {
            let mut env = self.env.clone();
            let Ok(mut turn) = env.reset(seed) else {
                continue;
            };
            struct PendingStep {
                seat: Seat,
                features: Vec<f32>,
                legal: Vec<bool>,
                policy_target: Vec<f32>,
                reward: f64,
            }
            let mut pending: Vec<PendingStep> = Vec::new();
            let max_demo_steps = self.max_steps.max(500);
            let outcome = loop {
                match turn {
                    Turn::Done(o) => break o,
                    Turn::Decide(d) => {
                        if pending.len() >= max_demo_steps {
                            break Outcome {
                                completed: false,
                                end_time: portunus_core::SimTime::ZERO,
                                seats: Vec::new(),
                                pulls: Vec::new(),
                            };
                        }
                        let features = self.featurizer.featurize(&d.obs, &d.legal);
                        let legal = self.space.legal(&d.legal);
                        let choice = policy.act(&d);
                        let mut policy_target = vec![0.0f32; space_size];
                        let legal_count = legal.iter().filter(|&&b| b).count();
                        if let Some(chosen_idx) = self.space.encode(&choice, &d.legal) {
                            if legal_count > 1 {
                                let smooth = 0.005 / legal_count as f32;
                                for (i, &ok) in legal.iter().enumerate() {
                                    if ok {
                                        policy_target[i] = smooth;
                                    }
                                }
                                policy_target[chosen_idx] += 0.995;
                            } else {
                                policy_target[chosen_idx] = 1.0;
                            }
                        } else if legal_count > 0 {
                            let u = 1.0 / legal_count as f32;
                            for (i, &ok) in legal.iter().enumerate() {
                                if ok {
                                    policy_target[i] = u;
                                }
                            }
                        }
                        let Ok(tr) = env.step(choice) else {
                            break Outcome {
                                completed: false,
                                end_time: portunus_core::SimTime::ZERO,
                                seats: Vec::new(),
                                pulls: Vec::new(),
                            };
                        };
                        pending.push(PendingStep {
                            seat: d.seat,
                            features,
                            legal,
                            policy_target,
                            reward: tr.reward,
                        });
                        turn = tr.next;
                    }
                }
            };
            if pending.is_empty() {
                continue;
            }

            let mut running_return = 0.0f64;
            let mut samples: Vec<Sample> = Vec::with_capacity(pending.len());
            for step in pending.into_iter().rev() {
                running_return = step.reward + gamma * running_return;
                samples.push(Sample {
                    seat: step.seat,
                    features: step.features,
                    legal: step.legal,
                    policy_target: step.policy_target,
                    value_target: running_return as f32,
                });
            }
            samples.reverse();

            out.push(Trajectory {
                seed,
                model: version.clone(),
                schema: self.featurizer.schema().clone(),
                samples,
                outcome,
            });
        }
        out
    }
}

impl<E: Env + Send> Collector for SelfPlayCollector<E> {
    fn collect(&mut self, model: Arc<dyn Model>, seeds: &[Seed]) -> Vec<Trajectory> {
        let threads = self.threads.get().min(seeds.len().max(1));
        if threads <= 1 || seeds.len() <= 1 {
            return seeds
                .iter()
                .filter_map(|&seed| {
                    self.rollout_mcts_seed(self.env.clone(), Arc::clone(&model), seed)
                })
                .collect();
        }

        let mut slots: Vec<Option<Trajectory>> = (0..seeds.len()).map(|_| None).collect();
        let config = self.config;
        let discount = self.discount;
        let max_steps = self.max_steps;
        let threads_setting = self.threads;

        thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|t| {
                    let env = self.env.clone();
                    let space = Arc::clone(&self.space);
                    let featurizer = Arc::clone(&self.featurizer);
                    let model = Arc::clone(&model);
                    scope.spawn(move || {
                        let worker_collector = SelfPlayCollector {
                            env,
                            config,
                            space,
                            featurizer,
                            discount,
                            max_steps,
                            threads: threads_setting,
                        };
                        seeds
                            .iter()
                            .enumerate()
                            .skip(t)
                            .step_by(threads)
                            .filter_map(|(i, &seed)| {
                                worker_collector
                                    .rollout_mcts_seed(
                                        worker_collector.env.clone(),
                                        Arc::clone(&model),
                                        seed,
                                    )
                                    .map(|traj| (i, traj))
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();

            for worker in workers {
                let done = worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
                for (i, traj) in done {
                    slots[i] = Some(traj);
                }
            }
        });

        slots.into_iter().flatten().collect()
    }
}

/// Burn [`Trainer`] that optimizes a [`PolicyValueNet`] with automatic
/// differentiation, masked policy cross-entropy loss, value [`MseLoss`], and
/// [`AdamWConfig`].
pub struct BurnTrainer {
    pub version: ModelVersion,
    pub schema: SchemaId,
    pub config: PolicyValueNetConfig,
    pub device: Device,
    pub net: PolicyValueNet,
    pub optimizer: ModuleOptimizer,
    pub learning_rate: LearningRate,
    pub samples_seen: u64,
}

impl BurnTrainer {
    /// Creates a new [`BurnTrainer`] on `device` (enabling Burn's autodiff
    /// graph decorator) with an `AdamW` optimizer.
    pub fn new(
        version: ModelVersion,
        schema: SchemaId,
        config: PolicyValueNetConfig,
        learning_rate: LearningRate,
        weight_decay: f32,
        device: Device,
    ) -> Self {
        let ad_device = device.autodiff();
        let net = config.init(&ad_device);
        let optimizer = AdamWConfig::new().with_weight_decay(weight_decay).init();
        Self {
            version,
            schema,
            config,
            device: ad_device,
            net,
            optimizer,
            learning_rate,
            samples_seen: 0,
        }
    }

    /// Exports the current network in validation (inference) mode as a
    /// [`BurnModel`].
    pub fn to_burn_model(&self) -> BurnModel {
        BurnModel::from_net(
            self.version.clone(),
            self.schema.clone(),
            self.config.clone(),
            Device::default(),
            self.net.clone(),
        )
    }
}

impl Trainer for BurnTrainer {
    fn schema(&self) -> &SchemaId {
        &self.schema
    }

    fn train(&mut self, batch: &[Sample]) -> TrainStats {
        let in_w = self.config.input_width;
        let out_w = self.config.output_width;
        let valid: Vec<&Sample> = batch
            .iter()
            .filter(|s| {
                s.features.len() >= in_w && s.legal.len() >= out_w && s.policy_target.len() >= out_w
            })
            .collect();
        if valid.is_empty() {
            return TrainStats {
                policy_loss: 0.0,
                value_loss: 0.0,
                samples_seen: self.samples_seen,
            };
        }

        let b = valid.len();
        let mut flat_x = Vec::with_capacity(b * in_w);
        let mut flat_pi = Vec::with_capacity(b * out_w);
        let mut flat_illegal = Vec::with_capacity(b * out_w);
        let mut flat_v = Vec::with_capacity(b);

        for s in &valid {
            flat_x.extend_from_slice(&s.features[..in_w]);
            for a in 0..out_w {
                let is_legal = s.legal[a];
                flat_illegal.push(!is_legal);
                flat_pi.push(if is_legal {
                    s.policy_target[a].max(0.0)
                } else {
                    0.0
                });
            }
            flat_v.push(s.value_target);
        }

        let x = Tensor::<2>::from_data(TensorData::new(flat_x, [b, in_w]), &self.device);
        let target_pi = Tensor::<2>::from_data(TensorData::new(flat_pi, [b, out_w]), &self.device);
        let illegal_mask =
            Tensor::<2, Bool>::from_data(TensorData::new(flat_illegal, [b, out_w]), &self.device);
        let target_v = Tensor::<2>::from_data(TensorData::new(flat_v, [b, 1]), &self.device);

        let (logits, values) = self.net.forward(x);
        let masked_logits = logits.mask_fill(illegal_mask, -1e9);
        let log_probs = log_softmax(masked_logits, 1);

        let policy_loss = -(target_pi * log_probs).sum_dim(1).mean();
        let value_loss = MseLoss::new().forward(values, target_v, Reduction::Mean);
        let total_loss = policy_loss.clone() + value_loss.clone().mul_scalar(0.01);

        let p_loss_scalar: f32 = policy_loss
            .clone()
            .into_data()
            .try_to_vec()
            .ok()
            .and_then(|v: Vec<f32>| v.first().copied())
            .unwrap_or(0.0);
        let v_loss_scalar: f32 = value_loss
            .clone()
            .into_data()
            .try_to_vec()
            .ok()
            .and_then(|v: Vec<f32>| v.first().copied())
            .unwrap_or(0.0);

        let grads = GradientsParams::from_grads(total_loss.backward(), &self.net);
        self.net = self
            .optimizer
            .step(self.learning_rate, self.net.clone(), grads);

        self.version.step += 1;
        self.samples_seen += b as u64;

        TrainStats {
            policy_loss: p_loss_scalar,
            value_loss: v_loss_scalar,
            samples_seen: self.samples_seen,
        }
    }

    fn checkpoint(&mut self) -> Checkpoint {
        let model = self.to_burn_model();
        Checkpoint {
            version: self.version.clone(),
            schema: self.schema.clone(),
            weights: model.to_bytes().expect("BurnModel serializes to burnpack"),
        }
    }
}

/// In-memory [`ModelStore`] for [`BurnModel`] checkpoints.
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    checkpoints: BTreeMap<ModelVersion, Checkpoint>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ModelStore for MemoryStore {
    fn latest(&self, name: &str) -> Option<ModelVersion> {
        self.checkpoints
            .keys()
            .filter(|v| v.name == name)
            .max_by_key(|v| v.step)
            .cloned()
    }

    fn load(&self, version: &ModelVersion) -> Result<Arc<dyn Model>, StoreError> {
        let ckpt = self
            .checkpoints
            .get(version)
            .ok_or_else(|| StoreError::Unknown(version.clone()))?;
        let model = BurnModel::from_bytes(&ckpt.weights, Device::default())
            .map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(Arc::new(model))
    }

    fn save(&mut self, checkpoint: &Checkpoint) -> Result<(), StoreError> {
        if self.checkpoints.contains_key(&checkpoint.version) {
            return Err(StoreError::Exists(checkpoint.version.clone()));
        }
        self.checkpoints
            .insert(checkpoint.version.clone(), checkpoint.clone());
        Ok(())
    }
}

/// Directory-backed [`ModelStore`] that persists [`BurnModel`] [`Checkpoint`]s
/// on disk.
#[derive(Debug, Clone)]
pub struct DirStore {
    dir: PathBuf,
}

impl DirStore {
    pub fn new(dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(Self { dir })
    }

    fn path_for(&self, version: &ModelVersion) -> PathBuf {
        self.dir
            .join(format!("{}__{}.ckpt", version.name, version.step))
    }
}

impl ModelStore for DirStore {
    fn latest(&self, name: &str) -> Option<ModelVersion> {
        let prefix = format!("{name}__");
        let entries = fs::read_dir(&self.dir).ok()?;
        entries
            .filter_map(|e| {
                let file_name = e.ok()?.file_name();
                let s = file_name.to_str()?;
                let rest = s.strip_prefix(&prefix)?.strip_suffix(".ckpt")?;
                let step: u64 = rest.parse().ok()?;
                Some(ModelVersion {
                    name: name.to_string(),
                    step,
                })
            })
            .max_by_key(|v| v.step)
    }

    fn load(&self, version: &ModelVersion) -> Result<Arc<dyn Model>, StoreError> {
        let path = self.path_for(version);
        if !path.exists() {
            return Err(StoreError::Unknown(version.clone()));
        }
        let bytes = fs::read(&path).map_err(|e| StoreError::Io(e.to_string()))?;
        let ckpt: Checkpoint =
            postcard::from_bytes(&bytes).map_err(|e| StoreError::Io(e.to_string()))?;
        let model = BurnModel::from_bytes(&ckpt.weights, Device::default())
            .map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(Arc::new(model))
    }

    fn save(&mut self, checkpoint: &Checkpoint) -> Result<(), StoreError> {
        let path = self.path_for(&checkpoint.version);
        if path.exists() {
            return Err(StoreError::Exists(checkpoint.version.clone()));
        }
        let bytes = postcard::to_allocvec(checkpoint).map_err(|e| StoreError::Io(e.to_string()))?;
        fs::write(&path, bytes).map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use portunus_core::{SimDuration, SimTime, SpellId};
    use portunus_engine::{ActionMask, Choice, Readiness, WakeReason};
    use portunus_env::{Decision, DecisionKey, EnvError, SpecActionSpace, Transition};
    use portunus_policy::{IdentityFeaturizer, ModelPolicy, Policy};

    use super::*;

    const STRIKE_A: SpellId = SpellId(10);
    const STRIKE_B: SpellId = SpellId(20);

    #[derive(Clone)]
    struct BanditEnv {
        done: bool,
    }

    impl Env for BanditEnv {
        type Obs = Vec<f32>;

        fn reset(&mut self, seed: Seed) -> Result<Turn<Self::Obs>, EnvError> {
            self.done = false;
            Ok(Turn::Decide(Decision {
                seat: Seat(0),
                reason: WakeReason::CombatStart,
                obs: vec![1.0, 0.5],
                legal: ActionMask {
                    abilities: BTreeMap::from([
                        (STRIKE_A, Readiness::Now),
                        (STRIKE_B, Readiness::Now),
                    ]),
                    can_stop: false,
                    can_wait_tick: false,
                    targets: Vec::new(),
                    cancel: Vec::new(),
                    can_move: false,
                    can_stop_move: false,
                },
                key: DecisionKey {
                    seed,
                    seat: Seat(0),
                    index: 0,
                },
            }))
        }

        fn step(&mut self, choice: Choice) -> Result<Transition<Self::Obs>, EnvError> {
            self.done = true;
            let reward = match choice {
                Choice::Cast { ability, .. } if ability == STRIKE_B => 10.0,
                _ => 1.0,
            };
            Ok(Transition {
                reward,
                elapsed: SimDuration(1000),
                next: Turn::Done(Outcome {
                    end_time: SimTime(1000),
                    completed: true,
                    pulls: Vec::new(),
                    seats: Vec::new(),
                }),
            })
        }
    }

    #[test]
    fn self_play_collector_burn_trainer_and_store_learn_preferred_action() {
        let space: Arc<dyn ActionSpace> =
            Arc::new(SpecActionSpace::from_spells([STRIKE_A, STRIKE_B], 0, 0));
        let schema = SchemaId("bandit:v1".into());
        let featurizer: Arc<dyn Featurizer<Vec<f32>>> = Arc::new(IdentityFeaturizer {
            schema: schema.clone(),
            width: 2,
        });
        let net_cfg = PolicyValueNetConfig::new(2, space.size()).with_hidden_width(16);

        let mut store = MemoryStore::new();
        let mut trainer = BurnTrainer::new(
            ModelVersion {
                name: "bandit".into(),
                step: 0,
            },
            schema,
            net_cfg,
            0.05,
            1e-4,
            Device::default(),
        );
        store.save(&trainer.checkpoint()).unwrap();

        let config = MctsConfig {
            simulations: 32,
            c_puct: 1.5,
            dirichlet_alpha: 0.3,
            dirichlet_frac: 0.0,
            temperature: 0.0,
            horizon: SimDuration(5000),
        };
        let mut collector = SelfPlayCollector::new(
            BanditEnv { done: false },
            config,
            Arc::clone(&space),
            Arc::clone(&featurizer),
        );

        let v0 = store.latest("bandit").unwrap();
        let m0 = store.load(&v0).unwrap();
        let trajs = collector.collect(m0, &[Seed(1), Seed(2), Seed(3), Seed(4)]);
        assert_eq!(trajs.len(), 4);
        let samples: Vec<Sample> = trajs.into_iter().flat_map(|t| t.samples).collect();
        assert_eq!(samples.len(), 4);
        for s in &samples {
            assert!((s.value_target - 10.0).abs() < 1e-5);
        }

        let first_stats = trainer.train(&samples);
        let mut last_stats = first_stats;
        for _ in 0..25 {
            last_stats = trainer.train(&samples);
        }
        assert!(
            last_stats.policy_loss < first_stats.policy_loss,
            "{last_stats:?} vs {first_stats:?}"
        );
        assert!(
            last_stats.value_loss < first_stats.value_loss,
            "{last_stats:?} vs {first_stats:?}"
        );

        let ckpt = trainer.checkpoint();
        store.save(&ckpt).unwrap();
        assert!(matches!(store.save(&ckpt), Err(StoreError::Exists(_))));
        let latest = store.latest("bandit").unwrap();
        assert_eq!(latest.step, 26);

        let trained_model = store.load(&latest).unwrap();
        let deploy = ModelPolicy {
            space: Arc::clone(&space),
            featurizer,
            model: trained_model,
            temperature: 0.0,
        };
        let mut env = BanditEnv { done: false };
        let Turn::Decide(d) = env.reset(Seed(99)).unwrap() else {
            unreachable!();
        };
        assert_eq!(deploy.act(&d), Choice::cast(STRIKE_B));
    }
}
