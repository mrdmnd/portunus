//! In-combat control.
//!
//! Two kinds of controller, split by what they can touch:
//!
//! - [`Policy`]: sees only its observation. Deployable — hand-written
//!   rules, the SimulationCraft-style baseline, or a Burn neural network on its
//!   own ([`BurnModel`] + [`ModelPolicy`]).
//! - [`SearchPolicy`]: also gets the env, so it can fork and play ahead.
//!   Used for training targets and as an upper bound; never deployable.
//!
//! A party is just a policy that dispatches on [`Decision::seat`].
//!
//! Policies are stateless: a choice is a function of the decision alone
//! (observation, mask, and [`portunus_env::DecisionKey`] for randomness).
//! Anything a policy wants to "remember" — a burst sequence, a commitment
//! to a plan — must exist in the world state as auras, cooldowns, or plan
//! windows.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use burn::nn::{Linear, LinearConfig, Relu};
use burn::prelude::*;
use burn::store::ModuleRecord;
use burn::tensor::{activation::softmax, Bytes};
use portunus_core::rng::{domain, unit, Purpose};
use portunus_core::{Seat, SimDuration};
use portunus_engine::{ActionMask, Choice, Wait};
use portunus_env::{ActionSpace, Decision, DecisionKey, Env, Turn};
use serde::{Deserialize, Serialize};
use thiserror::Error;

static MCTS_ENV_MICROS: AtomicU64 = AtomicU64::new(0);
static MCTS_NN_MICROS: AtomicU64 = AtomicU64::new(0);

/// Returns and resets accumulated `(env_sim_micros, model_eval_micros)` recorded
/// inside [`Mcts::search`].
pub fn take_mcts_profile_micros() -> (u64, u64) {
    (
        MCTS_ENV_MICROS.swap(0, Ordering::Relaxed),
        MCTS_NN_MICROS.swap(0, Ordering::Relaxed),
    )
}

/// Any `fn(&Decision<O>) -> Choice`, or a closure of that shape, is a
/// policy.
pub trait Policy<O>: Send + Sync {
    fn act(&self, decision: &Decision<O>) -> Choice;
}

impl<O, F> Policy<O> for F
where
    F: Fn(&Decision<O>) -> Choice + Send + Sync,
{
    fn act(&self, decision: &Decision<O>) -> Choice {
        self(decision)
    }
}

/// May cache work between calls (e.g. reuse a search tree), but caches
/// must be pure optimizations: the result is what an empty cache would
/// produce.
pub trait SearchPolicy<E: Env> {
    fn search(&mut self, env: &E, decision: &Decision<E::Obs>) -> SearchResult;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub choice: Choice,
    /// Visit share per action-space index; the policy training target.
    pub visits: Vec<f32>,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MctsConfig {
    pub simulations: u32,
    pub c_puct: f32,
    pub dirichlet_alpha: f32,
    pub dirichlet_frac: f32,
    pub temperature: f32,
    /// Stop expanding this far past the root.
    pub horizon: SimDuration,
}

impl Default for MctsConfig {
    fn default() -> Self {
        Self {
            simulations: 96,
            c_puct: 1.5,
            dirichlet_alpha: 0.3,
            dirichlet_frac: 0.25,
            temperature: 1.0,
            horizon: SimDuration(10_000),
        }
    }
}

/// Names an observation encoding and action space together. A model only
/// makes sense on the schema it was trained under, and samples from
/// different schemas must never be mixed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SchemaId(pub String);

/// A trained network: features in, action priors and a value out.
pub trait Model: Send + Sync {
    fn version(&self) -> ModelVersion;
    fn schema(&self) -> &SchemaId;
    fn input_width(&self) -> usize;
    fn output_width(&self) -> usize;
    fn evaluate(&self, batch: &[&[f32]]) -> Vec<Evaluation>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Evaluation {
    pub priors: Vec<f32>,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ModelVersion {
    pub name: String,
    pub step: u64,
}

/// Builds the policy for each seat. Policies are stateless, so one build
/// can serve any number of parallel rollouts.
pub trait PolicyFactory<O>: Send + Sync {
    fn name(&self) -> &str;
    fn build(&self, seat: Seat, model: Option<Arc<dyn Model>>) -> Arc<dyn Policy<O>>;
}

/// Encodes a structured or flat observation into a fixed-width `f32` vector
/// matching [`Model::input_width`].
pub trait Featurizer<O>: Send + Sync {
    fn schema(&self) -> &SchemaId;
    fn width(&self) -> usize;
    fn featurize(&self, obs: &O, mask: &ActionMask) -> Vec<f32>;
}

/// Pass-through [`Featurizer`] when the observation is already `Vec<f32>`
/// (such as [`portunus_env::FlatObserver`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityFeaturizer {
    pub schema: SchemaId,
    pub width: usize,
}

impl Featurizer<Vec<f32>> for IdentityFeaturizer {
    fn schema(&self) -> &SchemaId {
        &self.schema
    }

    fn width(&self) -> usize {
        self.width
    }

    fn featurize(&self, obs: &Vec<f32>, _mask: &ActionMask) -> Vec<f32> {
        let mut out = obs.clone();
        out.resize(self.width, 0.0);
        out
    }
}

/// Cold-start [`Model`] that predicts uniform action priors and zero value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniformModel {
    pub version: ModelVersion,
    pub schema: SchemaId,
    pub input_width: usize,
    pub output_width: usize,
}

impl Model for UniformModel {
    fn version(&self) -> ModelVersion {
        self.version.clone()
    }

    fn schema(&self) -> &SchemaId {
        &self.schema
    }

    fn input_width(&self) -> usize {
        self.input_width
    }

    fn output_width(&self) -> usize {
        self.output_width
    }

    fn evaluate(&self, batch: &[&[f32]]) -> Vec<Evaluation> {
        let p = if self.output_width > 0 {
            1.0 / self.output_width as f32
        } else {
            0.0
        };
        batch
            .iter()
            .map(|_| Evaluation {
                priors: vec![p; self.output_width],
                value: 0.0,
            })
            .collect()
    }
}

/// Architecture configuration for the two-headed Burn [`PolicyValueNet`].
#[derive(Config, Debug, PartialEq, Eq)]
pub struct PolicyValueNetConfig {
    pub input_width: usize,
    #[config(default = 64)]
    pub hidden_width: usize,
    #[config(default = 2)]
    pub num_layers: usize,
    pub output_width: usize,
}

/// Two-headed Burn [`Module`] with dedicated residual policy and value MLP
/// towers of configurable depth (`num_layers`).
#[derive(Module, Debug)]
pub struct PolicyValueNet {
    pub policy_hidden: Linear,
    pub policy_blocks: Vec<Linear>,
    pub policy_head: Linear,
    pub value_hidden: Linear,
    pub value_blocks: Vec<Linear>,
    pub value_head: Linear,
    pub activation: Relu,
}

impl PolicyValueNetConfig {
    /// Initializes a [`PolicyValueNet`] on `device` with deterministic
    /// Kaiming-uniform weights keyed by [`portunus_core::rng`].
    pub fn init(&self, device: &Device) -> PolicyValueNet {
        let extra = self.num_layers.max(1).saturating_sub(1);
        PolicyValueNet {
            policy_hidden: det_linear(
                "policy_hidden",
                self.input_width,
                self.hidden_width,
                1.0,
                device,
            ),
            policy_blocks: (0..extra)
                .map(|i| {
                    det_linear(
                        &format!("policy_block_{i}"),
                        self.hidden_width,
                        self.hidden_width,
                        0.5,
                        device,
                    )
                })
                .collect(),
            policy_head: det_linear(
                "policy_head",
                self.hidden_width,
                self.output_width,
                0.01,
                device,
            ),
            value_hidden: det_linear(
                "value_hidden",
                self.input_width,
                self.hidden_width,
                1.0,
                device,
            ),
            value_blocks: (0..extra)
                .map(|i| {
                    det_linear(
                        &format!("value_block_{i}"),
                        self.hidden_width,
                        self.hidden_width,
                        0.5,
                        device,
                    )
                })
                .collect(),
            value_head: det_linear("value_head", self.hidden_width, 1, 0.01, device),
            activation: Relu::new(),
        }
    }
}

fn det_linear(name: &str, d_in: usize, d_out: usize, scale: f64, device: &Device) -> Linear {
    use burn::module::Param;
    use portunus_core::Seed;

    let mut layer = LinearConfig::new(d_in, d_out).init(device);
    let dom_w = domain(Purpose::Policy, &["net_init_w", name]);
    let dom_b = domain(Purpose::Policy, &["net_init_b", name]);
    let bound = (3.0 / (d_in.max(1) as f64)).sqrt() * scale;
    let w_data: Vec<f32> = (0..(d_in * d_out))
        .map(|i| ((2.0 * unit(Seed(42), dom_w, i as u64) - 1.0) * bound) as f32)
        .collect();
    let b_bound = if scale < 0.1 {
        0.0
    } else {
        (1.0 / (d_in.max(1) as f64)).sqrt() * scale
    };
    let b_data: Vec<f32> = (0..d_out)
        .map(|i| ((2.0 * unit(Seed(42), dom_b, i as u64) - 1.0) * b_bound) as f32)
        .collect();
    let req_grad = layer.weight.is_require_grad();
    let w_tensor = Tensor::<2>::from_data(TensorData::new(w_data, [d_in, d_out]), device);
    layer.weight = Param::initialized(
        layer.weight.id,
        if req_grad {
            w_tensor.require_grad()
        } else {
            w_tensor
        },
    );
    if let Some(prev_b) = &layer.bias {
        let b_tensor = Tensor::<1>::from_data(TensorData::new(b_data, [d_out]), device);
        layer.bias = Some(Param::initialized(
            prev_b.id,
            if req_grad {
                b_tensor.require_grad()
            } else {
                b_tensor
            },
        ));
    }
    layer
}

impl PolicyValueNet {
    /// Forward pass returning `(policy_logits, value)` of shapes
    /// `[batch, output_width]` and `[batch, 1]`.
    pub fn forward(&self, input: Tensor<2>) -> (Tensor<2>, Tensor<2>) {
        let mut ph = self
            .activation
            .forward(self.policy_hidden.forward(input.clone()));
        for block in &self.policy_blocks {
            ph = self.activation.forward(block.forward(ph.clone())) + ph;
        }
        let logits = self.policy_head.forward(ph);

        let mut vh = self.activation.forward(self.value_hidden.forward(input));
        for block in &self.value_blocks {
            vh = self.activation.forward(block.forward(vh.clone())) + vh;
        }
        let value = self.value_head.forward(vh);
        (logits, value)
    }
}

#[derive(Debug, Error)]
pub enum BurnModelError {
    #[error("burn record: {0}")]
    Record(String),
    #[error("serialize: {0}")]
    Serialize(#[from] postcard::Error),
}

#[derive(Serialize, Deserialize)]
struct BurnModelBundle {
    version: ModelVersion,
    schema: SchemaId,
    config: PolicyValueNetConfig,
    record_bytes: Vec<u8>,
}

/// Burn-backed [`Model`] wrapping a [`PolicyValueNet`] in inference mode.
#[derive(Debug, Clone)]
pub struct BurnModel {
    pub version: ModelVersion,
    pub schema: SchemaId,
    pub config: PolicyValueNetConfig,
    pub device: Device,
    pub net: PolicyValueNet,
}

impl BurnModel {
    /// Initializes a fresh [`BurnModel`] on `device` in validation (inference)
    /// mode.
    pub fn new(
        version: ModelVersion,
        schema: SchemaId,
        config: PolicyValueNetConfig,
        device: Device,
    ) -> Self {
        let net = config.init(&device).valid();
        Self {
            version,
            schema,
            config,
            device,
            net,
        }
    }

    /// Wraps an already-constructed [`PolicyValueNet`], switching it to
    /// validation mode.
    pub fn from_net(
        version: ModelVersion,
        schema: SchemaId,
        config: PolicyValueNetConfig,
        device: Device,
        net: PolicyValueNet,
    ) -> Self {
        Self {
            version,
            schema,
            config,
            device,
            net: net.valid(),
        }
    }

    /// Serializes the model metadata and Burn [`ModuleRecord`] (`burnpack`
    /// format) into a self-contained byte buffer.
    pub fn to_bytes(&self) -> Result<Vec<u8>, BurnModelError> {
        let record_bytes = self
            .net
            .clone()
            .into_record()
            .into_bytes()
            .map_err(|e| BurnModelError::Record(e.to_string()))?
            .to_vec();
        let bundle = BurnModelBundle {
            version: self.version.clone(),
            schema: self.schema.clone(),
            config: self.config.clone(),
            record_bytes,
        };
        Ok(postcard::to_allocvec(&bundle)?)
    }

    /// Reconstructs a [`BurnModel`] on `device` from bytes produced by
    /// [`Self::to_bytes`].
    pub fn from_bytes(bytes: &[u8], device: Device) -> Result<Self, BurnModelError> {
        let bundle: BurnModelBundle = postcard::from_bytes(bytes)?;
        let record = ModuleRecord::from_bytes(Bytes::from_bytes_vec(bundle.record_bytes))
            .map_err(|e| BurnModelError::Record(e.to_string()))?;
        let net = bundle
            .config
            .init(&device)
            .valid()
            .try_load_record(record)
            .map_err(|e| BurnModelError::Record(e.to_string()))?;
        Ok(Self {
            version: bundle.version,
            schema: bundle.schema,
            config: bundle.config,
            device,
            net,
        })
    }
}

impl Model for BurnModel {
    fn version(&self) -> ModelVersion {
        self.version.clone()
    }

    fn schema(&self) -> &SchemaId {
        &self.schema
    }

    fn input_width(&self) -> usize {
        self.config.input_width
    }

    fn output_width(&self) -> usize {
        self.config.output_width
    }

    fn evaluate(&self, batch: &[&[f32]]) -> Vec<Evaluation> {
        if batch.is_empty() {
            return Vec::new();
        }
        let b = batch.len();
        let in_w = self.config.input_width;
        let out_w = self.config.output_width;
        let mut flat = Vec::with_capacity(b * in_w);
        for row in batch {
            let take = row.len().min(in_w);
            flat.extend_from_slice(&row[..take]);
            if take < in_w {
                flat.resize(flat.len() + (in_w - take), 0.0);
            }
        }
        let x = Tensor::<2>::from_data(TensorData::new(flat, [b, in_w]), &self.device);
        let (logits, values) = self.net.forward(x);
        let probs = softmax(logits, 1);
        let probs_vec: Vec<f32> = probs
            .into_data()
            .try_to_vec()
            .unwrap_or_else(|_| vec![1.0 / out_w.max(1) as f32; b * out_w]);
        let values_vec: Vec<f32> = values
            .into_data()
            .try_to_vec()
            .unwrap_or_else(|_| vec![0.0; b]);

        (0..b)
            .map(|i| Evaluation {
                priors: probs_vec[i * out_w..(i + 1) * out_w].to_vec(),
                value: values_vec.get(i).copied().unwrap_or(0.0),
            })
            .collect()
    }
}

/// Deployable observation-only [`Policy`] backed by a [`Model`].
pub struct ModelPolicy<O> {
    pub space: Arc<dyn ActionSpace>,
    pub featurizer: Arc<dyn Featurizer<O>>,
    pub model: Arc<dyn Model>,
    pub temperature: f32,
}

impl<O> Policy<O> for ModelPolicy<O> {
    fn act(&self, decision: &Decision<O>) -> Choice {
        let features = self.featurizer.featurize(&decision.obs, &decision.legal);
        let eval = self
            .model
            .evaluate(&[&features])
            .into_iter()
            .next()
            .unwrap_or(Evaluation {
                priors: vec![1.0; self.space.size()],
                value: 0.0,
            });
        let legal = self.space.legal(&decision.legal);
        let legal_indices: Vec<usize> = legal
            .iter()
            .enumerate()
            .filter_map(|(i, &ok)| ok.then_some(i))
            .collect();
        if legal_indices.is_empty() {
            return Choice::Wait(Wait::NextEvent);
        }
        let chosen_idx = select_by_temperature(
            &legal_indices,
            &eval.priors,
            self.temperature,
            decision.key,
            0,
        );
        self.space
            .decode(chosen_idx, &decision.legal)
            .unwrap_or(Choice::Wait(Wait::NextEvent))
    }
}

/// [`PolicyFactory`] that builds [`ModelPolicy`] instances for each seat.
pub struct ModelPolicyFactory<O> {
    pub name: String,
    pub space: Arc<dyn ActionSpace>,
    pub featurizer: Arc<dyn Featurizer<O>>,
    pub default_model: Arc<dyn Model>,
    pub temperature: f32,
}

impl<O: 'static> PolicyFactory<O> for ModelPolicyFactory<O> {
    fn name(&self) -> &str {
        &self.name
    }

    fn build(&self, _seat: Seat, model: Option<Arc<dyn Model>>) -> Arc<dyn Policy<O>> {
        Arc::new(ModelPolicy {
            space: Arc::clone(&self.space),
            featurizer: Arc::clone(&self.featurizer),
            model: model.unwrap_or_else(|| Arc::clone(&self.default_model)),
            temperature: self.temperature,
        })
    }
}

/// PUCT Monte Carlo Tree Search ([`SearchPolicy`]) over a cloneable [`Env`].
pub struct Mcts<O> {
    pub config: MctsConfig,
    pub space: Arc<dyn ActionSpace>,
    pub featurizer: Arc<dyn Featurizer<O>>,
    pub model: Arc<dyn Model>,
}

#[derive(Debug, Clone)]
struct Edge {
    action_idx: usize,
    choice: Choice,
    prior: f32,
    visits: u32,
    value_sum: f64,
    child: Option<usize>,
}

#[derive(Debug, Clone)]
struct Node {
    initial_value: f32,
    priors: Vec<f32>,
    edges: Vec<Edge>,
}

impl<O> Mcts<O> {
    pub fn new(
        config: MctsConfig,
        space: Arc<dyn ActionSpace>,
        featurizer: Arc<dyn Featurizer<O>>,
        model: Arc<dyn Model>,
    ) -> Self {
        Self {
            config,
            space,
            featurizer,
            model,
        }
    }

    fn build_node(&self, decision: &Decision<O>, eval: Evaluation, root_noise: bool) -> Node {
        let legal = self.space.legal(&decision.legal);
        let mut raw_edges: Vec<(usize, Choice, f32)> = Vec::new();
        for (idx, is_legal) in legal.into_iter().enumerate() {
            if !is_legal {
                continue;
            }
            if let Some(choice) = self.space.decode(idx, &decision.legal) {
                let p = eval.priors.get(idx).copied().unwrap_or(0.0).max(0.0);
                raw_edges.push((idx, choice, p));
            }
        }
        if raw_edges.is_empty() {
            raw_edges.push((0, Choice::Wait(Wait::NextEvent), 1.0));
        }

        let sum: f32 = raw_edges.iter().map(|(_, _, p)| *p).sum();
        if sum > 0.0 && sum.is_finite() {
            for (_, _, p) in &mut raw_edges {
                *p /= sum;
            }
        } else {
            let u = 1.0 / raw_edges.len() as f32;
            for (_, _, p) in &mut raw_edges {
                *p = u;
            }
        }

        if root_noise
            && raw_edges.len() > 1
            && self.config.dirichlet_frac > 0.0
            && self.config.dirichlet_alpha > 0.0
        {
            for (_, _, p) in &mut raw_edges {
                *p = p.max(1e-6).powf(0.75);
            }
            let t_sum: f32 = raw_edges.iter().map(|(_, _, p)| *p).sum();
            if t_sum > 0.0 && t_sum.is_finite() {
                for (_, _, p) in &mut raw_edges {
                    *p /= t_sum;
                }
            }
            let frac = self.config.dirichlet_frac.clamp(0.0, 1.0);
            let noise = sample_dirichlet(
                self.config.dirichlet_alpha,
                raw_edges.len(),
                decision.key,
                100,
            );
            for ((_, _, p), n) in raw_edges.iter_mut().zip(noise) {
                *p = (1.0 - frac) * *p + frac * n;
            }
        }

        let edges = raw_edges
            .into_iter()
            .map(|(action_idx, choice, prior)| Edge {
                action_idx,
                choice,
                prior,
                visits: 0,
                value_sum: 0.0,
                child: None,
            })
            .collect();

        Node {
            initial_value: eval.value,
            priors: eval.priors,
            edges,
        }
    }

    fn expand(&self, decision: &Decision<O>, root_noise: bool, nn_us: &mut u64) -> Node {
        let features = self.featurizer.featurize(&decision.obs, &decision.legal);
        let t_nn = Instant::now();
        let eval = self
            .model
            .evaluate(&[&features])
            .into_iter()
            .next()
            .unwrap_or(Evaluation {
                priors: vec![1.0; self.space.size()],
                value: 0.0,
            });
        *nn_us += u64::try_from(t_nn.elapsed().as_micros()).unwrap_or(0);
        self.build_node(decision, eval, root_noise)
    }

    fn quick_act(
        &self,
        decision: &Decision<O>,
        priors: &[f32],
        root_key: DecisionKey,
        substream: u64,
    ) -> Choice {
        let legal = self.space.legal(&decision.legal);
        let cast_indices: Vec<usize> = legal
            .iter()
            .enumerate()
            .filter_map(|(i, &ok)| {
                (ok && matches!(
                    self.space.decode(i, &decision.legal),
                    Some(Choice::Cast { .. })
                ))
                .then_some(i)
            })
            .collect();
        let legal_indices = if cast_indices.is_empty() {
            legal
                .iter()
                .enumerate()
                .filter_map(|(i, &ok)| ok.then_some(i))
                .collect()
        } else {
            cast_indices
        };
        if legal_indices.is_empty() {
            return Choice::Wait(Wait::NextEvent);
        }
        let chosen_idx = select_by_temperature(&legal_indices, priors, 0.15, root_key, substream);
        self.space
            .decode(chosen_idx, &decision.legal)
            .unwrap_or(Choice::Wait(Wait::NextEvent))
    }

    #[allow(clippy::too_many_arguments)]
    fn rollout<E: Env<Obs = O>>(
        &self,
        sim: &mut E,
        mut dec: Decision<O>,
        mut elapsed: SimDuration,
        priors: &[f32],
        root_key: DecisionKey,
        edge_visits: u32,
        initial_gcd_step: u64,
        first_cast: Option<Choice>,
        nn_us: &mut u64,
    ) -> (f64, SimDuration, bool, Option<f32>) {
        let mut total_reward = 0.0;
        let mut cur_priors: Option<Vec<f32>> = None;
        let mut last_val: Option<f32> = None;
        let rollout_cap = self.config.horizon.min(SimDuration(16_000));
        let mut gcd_step = initial_gcd_step;
        let mut sub_step = 0u64;
        let mut pending_first = first_cast;
        for _ in 0..48u64 {
            if self.config.horizon == SimDuration::ZERO || elapsed >= rollout_cap {
                return (total_reward, elapsed, false, last_val);
            }
            if sub_step == 0
                && ((gcd_step == 1 && initial_gcd_step == 0 && cur_priors.is_none())
                    || (gcd_step >= 3 && gcd_step.is_multiple_of(3)))
            {
                let feat = self.featurizer.featurize(&dec.obs, &dec.legal);
                let t_nn = Instant::now();
                let ev = self.model.evaluate(&[&feat]).into_iter().next();
                *nn_us += u64::try_from(t_nn.elapsed().as_micros()).unwrap_or(0);
                if let Some(ev) = ev {
                    last_val = Some(ev.value);
                    cur_priors = Some(ev.priors);
                }
            }
            let active_priors = cur_priors.as_deref().unwrap_or(priors);
            let substream = 100 + u64::from(edge_visits) * 97 + gcd_step * 8 + sub_step.min(7);
            let choice = pending_first
                .take()
                .unwrap_or_else(|| self.quick_act(&dec, active_priors, root_key, substream));
            let Ok(tr) = sim.step(choice) else {
                break;
            };
            let dt = tr.elapsed.min(SimDuration(2_500));
            if dt > SimDuration::ZERO {
                gcd_step += 1;
                sub_step = 0;
            } else {
                sub_step += 1;
            }
            let step_discount = (-0.04 * (f64::from(elapsed.millis()) / 1000.0)).exp();
            if elapsed + dt > rollout_cap && dt > SimDuration::ZERO {
                let rem = f64::from(rollout_cap.0.saturating_sub(elapsed.0));
                let full = f64::from(dt.millis()).max(1.0);
                total_reward += step_discount * tr.reward * (rem / full).clamp(0.0, 1.0);
                elapsed = rollout_cap;
                return (
                    total_reward,
                    elapsed,
                    matches!(tr.next, Turn::Done(_)),
                    last_val,
                );
            }
            total_reward += step_discount * tr.reward;
            elapsed = elapsed + dt;
            match tr.next {
                Turn::Done(_) => return (total_reward, elapsed, true, None),
                Turn::Decide(next_dec) => dec = next_dec,
            }
        }
        (total_reward, elapsed, false, last_val)
    }
}

impl<E: Env> SearchPolicy<E> for Mcts<E::Obs> {
    fn search(&mut self, env: &E, decision: &Decision<E::Obs>) -> SearchResult {
        let t_total = Instant::now();
        let mut nn_us = 0u64;
        let mut nodes = vec![self.expand(decision, true, &mut nn_us)];
        let c_puct = f64::from(self.config.c_puct);
        let mut min_q = f64::INFINITY;
        let mut max_q = f64::NEG_INFINITY;

        let root_gumbels = sample_gumbel(
            nodes[0].edges.len(),
            decision.key,
            f64::from(self.config.dirichlet_frac) * 0.35,
        );

        type SimPath = Vec<(usize, usize, f64)>;

        struct PendingLeaf<E: Env> {
            parent_node: usize,
            edge_pos: usize,
            sim: E,
            next_dec: Decision<E::Obs>,
            features: Vec<f32>,
            elapsed_total: SimDuration,
            gcd_steps: u64,
            reached_horizon: bool,
            path: SimPath,
        }

        const WAVE_CAP: usize = 8;
        let mut sim_idx = 0u32;
        while sim_idx < self.config.simulations {
            let mut pending_leaves: Vec<PendingLeaf<E>> = Vec::with_capacity(WAVE_CAP);
            let mut completed_paths: Vec<(SimPath, f64)> = Vec::with_capacity(WAVE_CAP);

            while sim_idx < self.config.simulations
                && pending_leaves.len() + completed_paths.len() < WAVE_CAP
            {
                let mut sim = env.clone();
                let mut node_id = 0usize;
                let mut cur_dec: Option<Decision<E::Obs>> = None;
                let mut elapsed_total = SimDuration::ZERO;
                let mut gcd_steps = 0u64;
                let mut path: Vec<(usize, usize, f64)> = Vec::new();
                let mut collided = false;

                enum SimStep<E: Env> {
                    Done(f64),
                    Expand {
                        parent_node: usize,
                        edge_pos: usize,
                        sim: E,
                        next_dec: Decision<E::Obs>,
                        elapsed_total: SimDuration,
                        gcd_steps: u64,
                        reached_horizon: bool,
                    },
                }

                let step_outcome = loop {
                    let cur_legal_mask = if let Some(ref d_now) = cur_dec {
                        let mask = self.space.legal(&d_now.legal);
                        let node_mut = &mut nodes[node_id];
                        for (idx, &ok) in mask.iter().enumerate() {
                            if ok && !node_mut.edges.iter().any(|e| e.action_idx == idx) {
                                if let Some(choice) = self.space.decode(idx, &d_now.legal) {
                                    let prior =
                                        node_mut.priors.get(idx).copied().unwrap_or(0.01).max(1e-4);
                                    node_mut.edges.push(Edge {
                                        action_idx: idx,
                                        choice,
                                        prior,
                                        visits: 0,
                                        value_sum: 0.0,
                                        child: None,
                                    });
                                }
                            }
                        }
                        Some(mask)
                    } else {
                        None
                    };

                    let node = &nodes[node_id];
                    let total_visits: u32 = node.edges.iter().map(|e| e.visits).sum();
                    let sqrt_total = f64::from(total_visits.max(1)).sqrt();

                    let completed_v = {
                        let (mut w_sum, mut q_sum) = (0.0, 0.0);
                        for e in &node.edges {
                            if e.visits > 0 && max_q > min_q + 1e-6 {
                                let q = e.value_sum / f64::from(e.visits);
                                let nq = ((q - min_q) / (max_q - min_q)).clamp(0.0, 1.0);
                                let w = f64::from(e.visits);
                                w_sum += w;
                                q_sum += w * nq;
                            }
                        }
                        if w_sum > 0.0 {
                            q_sum / w_sum
                        } else {
                            0.5
                        }
                    };

                    let norm_q = |e: &Edge| -> f64 {
                        if e.visits == 0 {
                            completed_v
                        } else {
                            let q = e.value_sum / f64::from(e.visits);
                            if max_q > min_q + 1e-6 {
                                ((q - min_q) / (max_q - min_q)).clamp(0.0, 1.0)
                            } else {
                                0.5
                            }
                        }
                    };

                    let edge_pos =
                        if node_id == 0 && self.config.simulations >= 8 && node.edges.len() > 2 {
                            // Gumbel AlphaZero Sequential Halving at the root:
                            // narrow the active root candidate set across 3 halving stages.
                            let k_edges = node.edges.len();
                            let active_cap = if sim_idx < self.config.simulations / 2 {
                                k_edges.min(12)
                            } else if sim_idx < (self.config.simulations * 3) / 4 {
                                (k_edges.min(12) / 2).max(2)
                            } else {
                                (k_edges.min(12) / 4).max(2)
                            };
                            let mut ranked: Vec<(usize, f64)> = node
                                .edges
                                .iter()
                                .enumerate()
                                .map(|(idx, e)| {
                                    let logit = f64::from(e.prior.max(1e-6)).ln();
                                    let g = root_gumbels.get(idx).copied().unwrap_or(0.0);
                                    let nq = norm_q(e);
                                    (idx, logit + g + 3.5 * nq)
                                })
                                .collect();
                            ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
                            let survivors = &ranked[..active_cap.min(ranked.len())];
                            survivors
                                .iter()
                                .map(|&(idx, g_score)| {
                                    let e = &node.edges[idx];
                                    let u = norm_q(e)
                                        + c_puct * f64::from(e.prior) * sqrt_total
                                            / (1.0 + f64::from(e.visits))
                                        + 0.08 * g_score / (1.0 + f64::from(e.visits));
                                    (idx, u)
                                })
                                .max_by(|a, b| a.1.total_cmp(&b.1))
                                .map(|(idx, _)| idx)
                                .unwrap_or(0)
                        } else {
                            node.edges
                                .iter()
                                .enumerate()
                                .filter(|(_, e)| {
                                    cur_legal_mask.as_ref().is_none_or(|m| {
                                        m.get(e.action_idx).copied().unwrap_or(false)
                                    })
                                })
                                .max_by(|(_, a), (_, b)| {
                                    let qa = norm_q(a);
                                    let qb = norm_q(b);
                                    let ua = qa
                                        + c_puct * f64::from(a.prior) * sqrt_total
                                            / (1.0 + f64::from(a.visits));
                                    let ub = qb
                                        + c_puct * f64::from(b.prior) * sqrt_total
                                            / (1.0 + f64::from(b.visits));
                                    ua.total_cmp(&ub)
                                })
                                .map(|(idx, _)| idx)
                                .unwrap_or(0)
                        };

                    if nodes[node_id].edges[edge_pos].child.is_none()
                        && pending_leaves
                            .iter()
                            .any(|p| p.parent_node == node_id && p.edge_pos == edge_pos)
                    {
                        collided = true;
                        break SimStep::Done(0.0);
                    }

                    if node_id == 0 {
                        let root_visit_k = nodes[0].edges[edge_pos].visits;
                        sim.reseed(open_loop_seed(decision.key, root_visit_k));
                    }

                    let action_idx = nodes[node_id].edges[edge_pos].action_idx;
                    let choice = if let Some(ref d_now) = cur_dec {
                        self.space
                            .decode(action_idx, &d_now.legal)
                            .unwrap_or_else(|| nodes[node_id].edges[edge_pos].choice.clone())
                    } else {
                        nodes[node_id].edges[edge_pos].choice.clone()
                    };
                    let Ok(tr) = sim.step(choice) else {
                        break SimStep::Done(0.0);
                    };
                    if tr.elapsed > SimDuration::ZERO {
                        gcd_steps += 1;
                    }
                    elapsed_total = elapsed_total + tr.elapsed.min(SimDuration(2_500));
                    nodes[node_id].edges[edge_pos].visits += 1;
                    path.push((node_id, edge_pos, tr.reward));

                    match tr.next {
                        Turn::Done(_) => break SimStep::Done(0.0),
                        Turn::Decide(next_dec) => {
                            let reached_horizon = self.config.horizon > SimDuration::ZERO
                                && elapsed_total >= self.config.horizon;
                            if let Some(child_id) = nodes[node_id].edges[edge_pos].child {
                                if reached_horizon {
                                    let tree_r: f64 = path.iter().map(|(_, _, r)| *r).sum();
                                    let h_ms = f64::from(self.config.horizon.millis());
                                    let e_ms = f64::from(elapsed_total.millis()).max(1.0);
                                    break SimStep::Done(tree_r * (h_ms / e_ms) - tree_r);
                                }
                                node_id = child_id;
                                cur_dec = Some(next_dec);
                            } else {
                                break SimStep::Expand {
                                    parent_node: node_id,
                                    edge_pos,
                                    sim,
                                    next_dec,
                                    elapsed_total,
                                    gcd_steps,
                                    reached_horizon,
                                };
                            }
                        }
                    }
                };

                if collided {
                    for &(nid, epos, _) in &path {
                        nodes[nid].edges[epos].visits -= 1;
                    }
                    break;
                }

                match step_outcome {
                    SimStep::Done(leaf_value) => {
                        completed_paths.push((path, leaf_value));
                    }
                    SimStep::Expand {
                        parent_node,
                        edge_pos,
                        sim,
                        next_dec,
                        elapsed_total,
                        gcd_steps,
                        reached_horizon,
                    } => {
                        let features = self.featurizer.featurize(&next_dec.obs, &next_dec.legal);
                        pending_leaves.push(PendingLeaf {
                            parent_node,
                            edge_pos,
                            sim,
                            next_dec,
                            features,
                            elapsed_total,
                            gcd_steps,
                            reached_horizon,
                            path,
                        });
                    }
                }
                sim_idx += 1;
            }

            if !pending_leaves.is_empty() {
                pending_leaves
                    .sort_by_key(|l| l.next_dec.reason == portunus_engine::WakeReason::FreeAction);
                let batch_refs: Vec<&[f32]> = pending_leaves
                    .iter()
                    .map(|l| l.features.as_slice())
                    .collect();
                let t_nn = Instant::now();
                let evals_vec = self.model.evaluate(&batch_refs);
                nn_us += u64::try_from(t_nn.elapsed().as_micros()).unwrap_or(0);
                let mut evals = evals_vec.into_iter();
                for mut leaf in pending_leaves {
                    let eval = evals.next().unwrap_or(Evaluation {
                        priors: vec![1.0; self.space.size()],
                        value: 0.0,
                    });
                    let child = self.build_node(&leaf.next_dec, eval, false);
                    let child_id = nodes.len();
                    nodes.push(child);
                    nodes[leaf.parent_node].edges[leaf.edge_pos].child = Some(child_id);
                    let root_edge_visits = leaf
                        .path
                        .first()
                        .map(|&(nid, epos, _)| nodes[nid].edges[epos].visits)
                        .unwrap_or(1);
                    let first_cast =
                        if leaf.next_dec.reason == portunus_engine::WakeReason::FreeAction {
                            nodes[leaf.parent_node]
                                .edges
                                .iter()
                                .filter(|e| matches!(e.choice, Choice::Cast { .. }) && e.visits > 0)
                                .max_by(|a, b| {
                                    let qa = a.value_sum / f64::from(a.visits);
                                    let qb = b.value_sum / f64::from(b.visits);
                                    qa.total_cmp(&qb)
                                })
                                .map(|e| e.choice.clone())
                        } else {
                            None
                        };
                    let (roll_r, final_elapsed, done, roll_val) = if leaf.reached_horizon {
                        (0.0, leaf.elapsed_total, false, None)
                    } else {
                        self.rollout(
                            &mut leaf.sim,
                            leaf.next_dec,
                            leaf.elapsed_total,
                            &nodes[child_id].priors,
                            decision.key,
                            root_edge_visits,
                            leaf.gcd_steps,
                            first_cast,
                            &mut nn_us,
                        )
                    };
                    let rollout_cap = self.config.horizon.min(SimDuration(16_000));
                    let mc_leaf = if !done
                        && self.config.horizon > SimDuration::ZERO
                        && final_elapsed > SimDuration::ZERO
                    {
                        let tree_r: f64 = leaf.path.iter().map(|(_, _, r)| *r).sum();
                        let raw_total = tree_r + roll_r;
                        let h_ms = f64::from(self.config.horizon.millis());
                        let e_ms = f64::from(final_elapsed.max(rollout_cap).millis()).max(1.0);
                        raw_total * (h_ms / e_ms) - tree_r
                    } else {
                        roll_r
                    };
                    let v_pred = f64::from(roll_val.unwrap_or(nodes[child_id].initial_value));
                    let leaf_value = if !done && v_pred.is_finite() && v_pred.abs() > 1e-3 {
                        0.78 * mc_leaf + 0.22 * v_pred
                    } else {
                        mc_leaf
                    };
                    let mut g = leaf_value;
                    for (nid, epos, reward) in leaf.path.into_iter().rev() {
                        g += reward;
                        let edge = &mut nodes[nid].edges[epos];
                        edge.value_sum += g;
                        let q = edge.value_sum / f64::from(edge.visits);
                        min_q = min_q.min(q);
                        max_q = max_q.max(q);
                    }
                }
            }

            for (path, leaf_value) in completed_paths {
                let mut g = leaf_value;
                for (nid, epos, reward) in path.into_iter().rev() {
                    g += reward;
                    let edge = &mut nodes[nid].edges[epos];
                    edge.value_sum += g;
                    let q = edge.value_sum / f64::from(edge.visits);
                    min_q = min_q.min(q);
                    max_q = max_q.max(q);
                }
            }
        }

        let root = &nodes[0];
        let total_visits: u32 = root.edges.iter().map(|e| e.visits).sum();
        let max_visits: u32 = root.edges.iter().map(|e| e.visits).max().unwrap_or(0);
        let mut visits = vec![0.0f32; self.space.size()];
        if total_visits > 0 {
            let mut weights: Vec<(usize, f64)> = Vec::with_capacity(root.edges.len());
            for e in &root.edges {
                if e.visits > 0 && e.action_idx < visits.len() {
                    let q = e.value_sum / f64::from(e.visits);
                    let nq = if max_q > min_q + 1e-6 {
                        ((q - min_q) / (max_q - min_q)).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    if max_visits > 1 && e.visits == 1 && nq < 0.65 {
                        continue;
                    }
                    let w = f64::from(e.visits).sqrt() * (6.0 * nq).exp();
                    weights.push((e.action_idx, w));
                }
            }
            let w_sum: f64 = weights.iter().map(|(_, w)| *w).sum();
            if w_sum > 0.0 && w_sum.is_finite() {
                for (idx, w) in weights {
                    visits[idx] = (w / w_sum) as f32;
                }
            } else {
                for e in &root.edges {
                    if e.action_idx < visits.len() {
                        visits[e.action_idx] = e.visits as f32 / total_visits as f32;
                    }
                }
            }
        } else {
            for e in &root.edges {
                if e.action_idx < visits.len() {
                    visits[e.action_idx] = e.prior;
                }
            }
        }

        let value = if total_visits > 0 {
            let sum_v: f64 = root.edges.iter().map(|e| e.value_sum).sum();
            (sum_v / f64::from(total_visits)) as f32
        } else {
            root.initial_value
        };

        let chosen_edge = if self.config.temperature <= 1e-6 || root.edges.len() == 1 {
            let min_final_visits = (max_visits / 2).max(1);
            root.edges
                .iter()
                .max_by(|a, b| {
                    let a_final = a.visits >= min_final_visits;
                    let b_final = b.visits >= min_final_visits;
                    let qa = if a.visits > 0 {
                        a.value_sum / f64::from(a.visits)
                    } else {
                        f64::NEG_INFINITY
                    };
                    let qb = if b.visits > 0 {
                        b.value_sum / f64::from(b.visits)
                    } else {
                        f64::NEG_INFINITY
                    };
                    a_final
                        .cmp(&b_final)
                        .then_with(|| qa.total_cmp(&qb))
                        .then_with(|| a.visits.cmp(&b.visits))
                        .then_with(|| a.prior.total_cmp(&b.prior))
                })
                .expect("non-empty root edges")
        } else {
            let indices: Vec<usize> = (0..root.edges.len()).collect();
            let weights: Vec<f32> = root
                .edges
                .iter()
                .map(|e| visits.get(e.action_idx).copied().unwrap_or(e.prior))
                .collect();
            let pos =
                select_by_temperature(&indices, &weights, self.config.temperature, decision.key, 1);
            &root.edges[pos]
        };

        let total_us = u64::try_from(t_total.elapsed().as_micros()).unwrap_or(0);
        let env_us = total_us.saturating_sub(nn_us);
        MCTS_ENV_MICROS.fetch_add(env_us, Ordering::Relaxed);
        MCTS_NN_MICROS.fetch_add(nn_us, Ordering::Relaxed);

        SearchResult {
            choice: chosen_edge.choice.clone(),
            visits,
            value,
        }
    }
}

/// Deterministic open-loop future seed for the `visit_k`-th rollout of a
/// root action at `key`.
///
/// Distinct from `key.seed` so search cannot peek at the ground-truth
/// episode's future random draws (non-clairvoyance), while shared across root
/// actions at the same `visit_k` (Common Random Numbers variance reduction).
pub fn open_loop_seed(key: DecisionKey, visit_k: u32) -> portunus_core::Seed {
    let seat_name = format!("seat{}", key.seat.0);
    let dom = domain(Purpose::Policy, &[&seat_name, "open_loop"]);
    let draw_idx = key
        .index
        .wrapping_mul(65_536)
        .wrapping_add(u64::from(visit_k));
    let u = unit(key.seed, dom, draw_idx);
    let raw = (u * (u64::MAX as f64)) as u64 ^ 0x9E37_79B9_7F4A_7C15u64.wrapping_mul(draw_idx + 1);
    portunus_core::Seed(if raw == key.seed.0 { raw ^ 1 } else { raw })
}

fn select_by_temperature(
    candidates: &[usize],
    weights: &[f32],
    temperature: f32,
    key: DecisionKey,
    substream: u64,
) -> usize {
    if candidates.len() == 1 {
        return candidates[0];
    }
    if temperature <= 1e-6 {
        return *candidates
            .iter()
            .max_by(|&&a, &&b| {
                let wa = weights.get(a).copied().unwrap_or(0.0);
                let wb = weights.get(b).copied().unwrap_or(0.0);
                wa.total_cmp(&wb).then_with(|| b.cmp(&a))
            })
            .unwrap();
    }
    let inv_t = (1.0 / f64::from(temperature)).clamp(0.05, 20.0);
    let scaled: Vec<f64> = candidates
        .iter()
        .map(|&i| {
            let w = f64::from(weights.get(i).copied().unwrap_or(0.0).max(0.0));
            if w > 0.0 {
                w.powf(inv_t)
            } else {
                0.0
            }
        })
        .collect();
    let sum: f64 = scaled.iter().sum();
    if sum <= 0.0 || !sum.is_finite() {
        return candidates[0];
    }
    let seat_name = format!("seat{}", key.seat.0);
    let dom = domain(Purpose::Policy, &[&seat_name, "select"]);
    let u = unit(
        key.seed,
        dom,
        key.index.wrapping_mul(1024).wrapping_add(substream),
    ) * sum;
    let mut acc = 0.0;
    for (&cand, &w) in candidates.iter().zip(&scaled) {
        acc += w;
        if u < acc {
            return cand;
        }
    }
    *candidates.last().unwrap()
}

/// Deterministic Gumbel(`0, scale`) draws of length `k` keyed by `key`.
fn sample_gumbel(k: usize, key: DecisionKey, scale: f64) -> Vec<f64> {
    if scale <= 1e-9 || k == 0 {
        return vec![0.0; k];
    }
    let seat_name = format!("seat{}", key.seat.0);
    let dom = domain(Purpose::Policy, &[&seat_name, "gumbel"]);
    let base = key.index.wrapping_mul(65_536).wrapping_add(200);
    (0..k)
        .map(|i| {
            let u = unit(key.seed, dom, base.wrapping_add(i as u64)).clamp(1e-6, 1.0 - 1e-6);
            -(-u.ln()).ln() * scale
        })
        .collect()
}

/// Deterministic Dirichlet(`alpha`) draw of length `k` keyed by `key`.
fn sample_dirichlet(alpha: f32, k: usize, key: DecisionKey, offset: u64) -> Vec<f32> {
    let seat_name = format!("seat{}", key.seat.0);
    let dom = domain(Purpose::Policy, &[&seat_name, "dirichlet"]);
    let mut counter = key.index.wrapping_mul(65_536).wrapping_add(offset);
    let mut draw = || {
        let u = unit(key.seed, dom, counter).clamp(1e-12, 1.0 - 1e-12);
        counter += 1;
        u
    };
    let a = f64::from(alpha).max(1e-3);
    let mut gammas = Vec::with_capacity(k);
    for _ in 0..k {
        gammas.push(sample_gamma(a, &mut draw));
    }
    let sum: f64 = gammas.iter().sum();
    if sum <= 0.0 || !sum.is_finite() {
        return vec![1.0 / k as f32; k];
    }
    gammas.into_iter().map(|g| (g / sum) as f32).collect()
}

fn sample_gamma(alpha: f64, draw: &mut impl FnMut() -> f64) -> f64 {
    if alpha < 1.0 {
        let u = draw();
        return sample_gamma(alpha + 1.0, draw) * u.powf(1.0 / alpha);
    }
    let d = alpha - 1.0 / 3.0;
    let c = 1.0 / (9.0 * d).sqrt();
    for _ in 0..64 {
        let u1 = draw();
        let u2 = draw();
        let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
        let v = 1.0 + c * z;
        if v <= 0.0 {
            continue;
        }
        let v3 = v * v * v;
        let u = draw();
        if u < 1.0 - 0.0331 * z * z * z * z || u.ln() < 0.5 * z * z + d * (1.0 - v3 + v3.ln()) {
            return d * v3;
        }
    }
    alpha
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use portunus_core::{Seed, SimTime, SpellId};
    use portunus_engine::{Outcome, Readiness, WakeReason};
    use portunus_env::{EnvError, SpecActionSpace, Transition};

    use super::*;

    const FAST_TRAP: SpellId = SpellId(1);
    const SETUP_BUFF: SpellId = SpellId(2);
    const PAYOFF: SpellId = SpellId(3);

    /// Two-step toy environment:
    /// - Step 0: `FAST_TRAP` gives +1.0 then 0.0 (total 1.0);
    ///   `SETUP_BUFF` gives 0.0 then enables `PAYOFF` (+5.0) at step 1.
    #[derive(Clone)]
    struct ToyEnv {
        step: u64,
        buffed: bool,
    }

    impl ToyEnv {
        fn decision(&self) -> Decision<Vec<f32>> {
            let mut abilities = BTreeMap::from([
                (FAST_TRAP, Readiness::Now),
                (SETUP_BUFF, Readiness::Now),
                (
                    PAYOFF,
                    if self.buffed {
                        Readiness::Now
                    } else {
                        Readiness::Blocked
                    },
                ),
            ]);
            if self.step > 0 {
                abilities.insert(SETUP_BUFF, Readiness::Blocked);
            }
            Decision {
                seat: Seat(0),
                reason: WakeReason::CombatStart,
                obs: vec![self.step as f32, if self.buffed { 1.0 } else { 0.0 }],
                legal: ActionMask {
                    abilities,
                    can_stop: false,
                    can_wait_tick: false,
                    targets: Vec::new(),
                    cancel: Vec::new(),
                    can_move: false,
                    can_stop_move: false,
                },
                key: DecisionKey {
                    seed: Seed(7),
                    seat: Seat(0),
                    index: self.step,
                },
            }
        }
    }

    impl Env for ToyEnv {
        type Obs = Vec<f32>;

        fn reset(&mut self, _seed: Seed) -> Result<Turn<Self::Obs>, EnvError> {
            self.step = 0;
            self.buffed = false;
            Ok(Turn::Decide(self.decision()))
        }

        fn step(&mut self, choice: Choice) -> Result<Transition<Self::Obs>, EnvError> {
            let reward = match (self.step, &choice) {
                (0, Choice::Cast { ability, .. }) if *ability == FAST_TRAP => 1.0,
                (0, Choice::Cast { ability, .. }) if *ability == SETUP_BUFF => {
                    self.buffed = true;
                    0.0
                }
                (1, Choice::Cast { ability, .. }) if *ability == PAYOFF && self.buffed => 5.0,
                _ => -0.5,
            };
            self.step += 1;
            let next = if self.step >= 2 {
                Turn::Done(Outcome {
                    end_time: SimTime(2000),
                    completed: true,
                    pulls: Vec::new(),
                    seats: Vec::new(),
                })
            } else {
                Turn::Decide(self.decision())
            };
            Ok(Transition {
                reward,
                elapsed: SimDuration(1000),
                next,
            })
        }
    }

    #[test]
    fn mcts_finds_two_step_setup_and_payoff_over_greedy_trap() {
        let space: Arc<dyn ActionSpace> = Arc::new(SpecActionSpace::from_spells(
            [FAST_TRAP, SETUP_BUFF, PAYOFF],
            0,
            0,
        ));
        let schema = SchemaId("toy:v1".into());
        let featurizer = Arc::new(IdentityFeaturizer {
            schema: schema.clone(),
            width: 2,
        });
        let model = Arc::new(UniformModel {
            version: ModelVersion {
                name: "uniform".into(),
                step: 0,
            },
            schema,
            input_width: 2,
            output_width: space.size(),
        });
        let config = MctsConfig {
            simulations: 64,
            c_puct: 1.5,
            dirichlet_alpha: 0.3,
            dirichlet_frac: 0.0,
            temperature: 0.0,
            horizon: SimDuration(10_000),
        };
        let mut mcts = Mcts::new(config, Arc::clone(&space), featurizer, model);
        let mut env = ToyEnv {
            step: 0,
            buffed: false,
        };
        let Turn::Decide(d0) = env.reset(Seed(7)).unwrap() else {
            panic!("expected decision");
        };
        let res0 = mcts.search(&env, &d0);
        assert_eq!(res0.choice, Choice::cast(SETUP_BUFF));
        let setup_idx = space.encode(&Choice::cast(SETUP_BUFF), &d0.legal).unwrap();
        let trap_idx = space.encode(&Choice::cast(FAST_TRAP), &d0.legal).unwrap();
        assert!(
            res0.visits[setup_idx] > res0.visits[trap_idx],
            "visits: {:?}",
            res0.visits
        );
    }

    #[test]
    fn burn_model_round_trips_through_burnpack_record() {
        let device = Device::default();
        let cfg = PolicyValueNetConfig::new(3, 4).with_hidden_width(8);
        let m = BurnModel::new(
            ModelVersion {
                name: "burn_net".into(),
                step: 3,
            },
            SchemaId("s".into()),
            cfg,
            device.clone(),
        );
        let sample_in = [1.0f32, 0.5, -1.0];
        let eval_before = m.evaluate(&[&sample_in]);
        assert_eq!(eval_before.len(), 1);
        assert!((eval_before[0].priors.iter().sum::<f32>() - 1.0).abs() < 1e-5);

        let bytes = m.to_bytes().unwrap();
        let decoded = BurnModel::from_bytes(&bytes, device).unwrap();
        assert_eq!(decoded.version(), m.version());
        assert_eq!(decoded.schema(), m.schema());
        let eval_after = decoded.evaluate(&[&sample_in]);
        assert_eq!(eval_before, eval_after);
    }

    #[test]
    fn open_loop_mcts_optimizes_expected_value_without_clairvoyance_on_lucky_seed() {
        const LUCKY_GAMBLE: SpellId = SpellId(10);
        const RELIABLE_STRIKE: SpellId = SpellId(20);
        const PROC_FINISHER: SpellId = SpellId(30);

        #[derive(Clone)]
        struct StochasticEnv {
            seed: Seed,
            step: u64,
            proc_active: bool,
        }

        impl StochasticEnv {
            fn roll(&self, sub: u64) -> f64 {
                let dom = domain(Purpose::Mechanics, &["stoch_env"]);
                unit(self.seed, dom, self.step * 16 + sub)
            }

            fn decision(&self) -> Decision<Vec<f32>> {
                let abilities = if self.step == 0 {
                    BTreeMap::from([
                        (LUCKY_GAMBLE, Readiness::Now),
                        (RELIABLE_STRIKE, Readiness::Now),
                        (PROC_FINISHER, Readiness::Blocked),
                    ])
                } else {
                    BTreeMap::from([
                        (LUCKY_GAMBLE, Readiness::Blocked),
                        (RELIABLE_STRIKE, Readiness::Now),
                        (
                            PROC_FINISHER,
                            if self.proc_active {
                                Readiness::Now
                            } else {
                                Readiness::Blocked
                            },
                        ),
                    ])
                };
                Decision {
                    seat: Seat(0),
                    reason: WakeReason::CombatStart,
                    obs: vec![self.step as f32, if self.proc_active { 1.0 } else { 0.0 }],
                    legal: ActionMask {
                        abilities,
                        can_stop: false,
                        can_wait_tick: false,
                        targets: Vec::new(),
                        cancel: Vec::new(),
                        can_move: false,
                        can_stop_move: false,
                    },
                    key: DecisionKey {
                        seed: self.seed,
                        seat: Seat(0),
                        index: self.step,
                    },
                }
            }
        }

        impl Env for StochasticEnv {
            type Obs = Vec<f32>;

            fn reset(&mut self, seed: Seed) -> Result<Turn<Self::Obs>, EnvError> {
                self.seed = seed;
                self.step = 0;
                self.proc_active = false;
                Ok(Turn::Decide(self.decision()))
            }

            fn step(&mut self, choice: Choice) -> Result<Transition<Self::Obs>, EnvError> {
                let reward = match (self.step, &choice) {
                    (0, Choice::Cast { ability, .. }) if *ability == LUCKY_GAMBLE => {
                        // 10% chance of +100 (EV = 10), 90% chance of 0.
                        if self.roll(0) < 0.10 {
                            100.0
                        } else {
                            0.0
                        }
                    }
                    (0, Choice::Cast { ability, .. }) if *ability == RELIABLE_STRIKE => {
                        // Guaranteed +40 at step 0, plus 50% chance to proc PROC_FINISHER (+30) at step 1.
                        self.proc_active = self.roll(1) < 0.50;
                        40.0
                    }
                    (1, Choice::Cast { ability, .. }) if *ability == PROC_FINISHER => {
                        if !self.proc_active {
                            return Err(EnvError::Finished);
                        }
                        30.0
                    }
                    (1, Choice::Cast { ability, .. }) if *ability == RELIABLE_STRIKE => 15.0,
                    _ => 0.0,
                };
                self.step += 1;
                let next = if self.step >= 2 {
                    Turn::Done(Outcome {
                        end_time: SimTime(2000),
                        completed: true,
                        pulls: Vec::new(),
                        seats: Vec::new(),
                    })
                } else {
                    Turn::Decide(self.decision())
                };
                Ok(Transition {
                    reward,
                    elapsed: SimDuration(1000),
                    next,
                })
            }

            fn reseed(&mut self, seed: Seed) {
                self.seed = seed;
            }
        }

        // Find a ground-truth seed where `LUCKY_GAMBLE` happens to hit its 10% jackpot on step 0.
        let lucky_seed = (1..1000u64)
            .map(Seed)
            .find(|&s| {
                let env = StochasticEnv {
                    seed: s,
                    step: 0,
                    proc_active: false,
                };
                env.roll(0) < 0.10
            })
            .expect("find lucky seed");

        let space: Arc<dyn ActionSpace> = Arc::new(SpecActionSpace::from_spells(
            [LUCKY_GAMBLE, RELIABLE_STRIKE, PROC_FINISHER],
            0,
            0,
        ));
        let schema = SchemaId("stoch:v1".into());
        let featurizer = Arc::new(IdentityFeaturizer {
            schema: schema.clone(),
            width: 2,
        });
        let model = Arc::new(UniformModel {
            version: ModelVersion {
                name: "uniform".into(),
                step: 0,
            },
            schema,
            input_width: 2,
            output_width: space.size(),
        });
        let mut mcts = Mcts::new(
            MctsConfig {
                simulations: 64,
                c_puct: 1.25,
                dirichlet_alpha: 0.3,
                dirichlet_frac: 0.0,
                temperature: 0.0,
                horizon: SimDuration(10_000),
            },
            Arc::clone(&space),
            featurizer,
            model,
        );

        let mut env = StochasticEnv {
            seed: lucky_seed,
            step: 0,
            proc_active: false,
        };
        let Turn::Decide(d0) = env.reset(lucky_seed).unwrap() else {
            panic!("expected decision");
        };

        // Even though `lucky_seed` would roll < 0.10 (+100) on `LUCKY_GAMBLE`, Open-Loop MCTS
        // reseeds each rollout and discovers `RELIABLE_STRIKE` (EV = 40 + 0.5*30 + 0.5*15 = 62.5)
        // beats `LUCKY_GAMBLE` (EV = 10 + 15 = 25.0), while handling stochastic `PROC_FINISHER`
        // legality at depth 1 across proc and non-proc worlds.
        let res = mcts.search(&env, &d0);
        assert_eq!(res.choice, Choice::cast(RELIABLE_STRIKE));
    }
}
