//! Self-play and training.
//!
//! The simulator side collects [`Trajectory`]s; a [`Trainer`] turns them
//! into new model versions held by a [`ModelStore`]. Samples are plain
//! numbers so the trainer can live in another language.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use portunus_core::{Seat, Seed};
use portunus_engine::Outcome;
use portunus_env::{ActionSpace, Env, Turn};
use portunus_policy::{
    Featurizer, LinearModel, Mcts, MctsConfig, Model, ModelVersion, SchemaId, SearchPolicy,
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

/// A trained model, serialized by the trainer in its own format.
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
        }
    }
}

impl<E: Env> Collector for SelfPlayCollector<E> {
    fn collect(&mut self, model: Arc<dyn Model>, seeds: &[Seed]) -> Vec<Trajectory> {
        let mut out = Vec::with_capacity(seeds.len());
        let mut mcts = Mcts::new(
            self.config,
            Arc::clone(&self.space),
            Arc::clone(&self.featurizer),
            Arc::clone(&model),
        );

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
            let outcome = loop {
                match turn {
                    Turn::Done(o) => break Some(o),
                    Turn::Decide(d) => {
                        if pending.len() >= self.max_steps {
                            break None;
                        }
                        let features = self.featurizer.featurize(&d.obs, &d.legal);
                        let legal = self.space.legal(&d.legal);
                        let sr = mcts.search(&env, &d);
                        let Ok(tr) = env.step(sr.choice) else {
                            break None;
                        };
                        pending.push(PendingStep {
                            seat: d.seat,
                            features,
                            legal,
                            policy_target: sr.visits,
                            reward: tr.reward,
                        });
                        turn = tr.next;
                    }
                }
            };
            let Some(outcome) = outcome else {
                continue;
            };

            let gamma = f64::from(self.discount.clamp(0.0, 1.0));
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
                model: model.version(),
                schema: self.featurizer.schema().clone(),
                samples,
                outcome,
            });
        }
        out
    }
}

/// Mini-batch gradient descent [`Trainer`] for a [`LinearModel`].
#[derive(Debug, Clone)]
pub struct LinearTrainer {
    pub model: LinearModel,
    pub learning_rate: f32,
    pub l2_reg: f32,
    pub samples_seen: u64,
}

impl LinearTrainer {
    pub fn new(model: LinearModel, learning_rate: f32, l2_reg: f32) -> Self {
        Self {
            model,
            learning_rate,
            l2_reg,
            samples_seen: 0,
        }
    }
}

impl Trainer for LinearTrainer {
    fn schema(&self) -> &SchemaId {
        &self.model.schema
    }

    fn train(&mut self, batch: &[Sample]) -> TrainStats {
        if batch.is_empty() {
            return TrainStats {
                policy_loss: 0.0,
                value_loss: 0.0,
                samples_seen: self.samples_seen,
            };
        }

        let in_w = self.model.input_width;
        let out_w = self.model.output_width;
        let mut grad_pw = vec![0.0f32; out_w * in_w];
        let mut grad_pb = vec![0.0f32; out_w];
        let mut grad_vw = vec![0.0f32; in_w];
        let mut grad_vb = 0.0f32;

        let mut total_policy_loss = 0.0f32;
        let mut total_value_loss = 0.0f32;
        let mut valid_count = 0usize;

        for sample in batch {
            if sample.features.len() < in_w
                || sample.legal.len() < out_w
                || sample.policy_target.len() < out_w
            {
                continue;
            }
            let x = &sample.features[..in_w];
            let logits = self.model.logits(x);

            // Masked softmax over legal actions.
            let max_z = logits
                .iter()
                .zip(&sample.legal[..out_w])
                .filter_map(|(&z, &ok)| ok.then_some(z))
                .fold(f32::NEG_INFINITY, f32::max);

            let mut probs = vec![0.0f32; out_w];
            if max_z.is_finite() {
                let mut sum_exp = 0.0f32;
                for a in 0..out_w {
                    if sample.legal[a] {
                        let e = (logits[a] - max_z).exp();
                        probs[a] = e;
                        sum_exp += e;
                    }
                }
                if sum_exp > 0.0 && sum_exp.is_finite() {
                    for p in &mut probs {
                        *p /= sum_exp;
                    }
                }
            }

            let mut p_loss = 0.0f32;
            for a in 0..out_w {
                if !sample.legal[a] {
                    continue;
                }
                let target_p = sample.policy_target[a].max(0.0);
                if target_p > 0.0 {
                    p_loss -= target_p * probs[a].max(1e-12).ln();
                }
                let dz = probs[a] - target_p;
                grad_pb[a] += dz;
                let row = &mut grad_pw[a * in_w..(a + 1) * in_w];
                for (g, &xi) in row.iter_mut().zip(x) {
                    *g += dz * xi;
                }
            }
            total_policy_loss += p_loss;

            let v_pred = self.model.predict_value(x);
            let diff = v_pred - sample.value_target;
            total_value_loss += diff * diff;
            let dv = 2.0 * diff;
            grad_vb += dv;
            for (g, &xi) in grad_vw.iter_mut().zip(x) {
                *g += dv * xi;
            }

            valid_count += 1;
        }

        if valid_count == 0 {
            return TrainStats {
                policy_loss: 0.0,
                value_loss: 0.0,
                samples_seen: self.samples_seen,
            };
        }

        let inv_n = 1.0 / valid_count as f32;
        let lr = self.learning_rate;
        let l2 = self.l2_reg;

        for (w, g) in self.model.policy_w.iter_mut().zip(&grad_pw) {
            *w -= lr * (g * inv_n + l2 * *w);
        }
        for (b, g) in self.model.policy_b.iter_mut().zip(&grad_pb) {
            *b -= lr * (g * inv_n);
        }
        for (w, g) in self.model.value_w.iter_mut().zip(&grad_vw) {
            *w -= lr * (g * inv_n + l2 * *w);
        }
        self.model.value_b -= lr * (grad_vb * inv_n);

        self.model.version.step += 1;
        self.samples_seen += valid_count as u64;

        TrainStats {
            policy_loss: total_policy_loss * inv_n,
            value_loss: total_value_loss * inv_n,
            samples_seen: self.samples_seen,
        }
    }

    fn checkpoint(&mut self) -> Checkpoint {
        Checkpoint {
            version: self.model.version.clone(),
            schema: self.model.schema.clone(),
            weights: self.model.to_bytes().expect("LinearModel serializes"),
        }
    }
}

/// In-memory [`ModelStore`] for [`LinearModel`] checkpoints.
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
        let model =
            LinearModel::from_bytes(&ckpt.weights).map_err(|e| StoreError::Io(e.to_string()))?;
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

/// Directory-backed [`ModelStore`] that persists [`Checkpoint`]s as
/// `postcard`-encoded files on disk.
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
        let model =
            LinearModel::from_bytes(&ckpt.weights).map_err(|e| StoreError::Io(e.to_string()))?;
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
    fn self_play_collector_trainer_and_store_learn_preferred_action() {
        let space: Arc<dyn ActionSpace> =
            Arc::new(SpecActionSpace::from_spells([STRIKE_A, STRIKE_B], 0, 0));
        let schema = SchemaId("bandit:v1".into());
        let featurizer: Arc<dyn Featurizer<Vec<f32>>> = Arc::new(IdentityFeaturizer {
            schema: schema.clone(),
            width: 2,
        });
        let init_model = LinearModel::zeros(
            ModelVersion {
                name: "bandit".into(),
                step: 0,
            },
            schema.clone(),
            2,
            space.size(),
        );

        let mut store = MemoryStore::new();
        let mut trainer = LinearTrainer::new(init_model, 0.25, 1e-4);
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
        for _ in 0..20 {
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
        assert_eq!(latest.step, 21);

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
