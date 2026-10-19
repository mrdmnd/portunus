//! In-combat control.
//!
//! Two kinds of controller, split by what they can touch:
//!
//! - [`Policy`]: sees only its observation. Deployable — hand-written
//!   rules, the SimulationCraft-style baseline, a Burn neural network on its
//!   own ([`BurnModel`] + [`ModelPolicy`]), or a distilled student can all
//!   run in an addon.
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

use std::sync::Arc;

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
            simulations: 64,
            c_puct: 1.5,
            dirichlet_alpha: 0.3,
            dirichlet_frac: 0.25,
            temperature: 1.0,
            horizon: SimDuration(15_000),
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
    pub output_width: usize,
}

/// Two-headed Burn [`Module`] with dedicated policy and value MLP towers.
#[derive(Module, Debug)]
pub struct PolicyValueNet {
    pub policy_hidden: Linear,
    pub policy_head: Linear,
    pub value_hidden: Linear,
    pub value_head: Linear,
    pub activation: Relu,
}

impl PolicyValueNetConfig {
    /// Initializes a [`PolicyValueNet`] on `device`.
    pub fn init(&self, device: &Device) -> PolicyValueNet {
        PolicyValueNet {
            policy_hidden: LinearConfig::new(self.input_width, self.hidden_width).init(device),
            policy_head: LinearConfig::new(self.hidden_width, self.output_width).init(device),
            value_hidden: LinearConfig::new(self.input_width, self.hidden_width).init(device),
            value_head: LinearConfig::new(self.hidden_width, 1).init(device),
            activation: Relu::new(),
        }
    }
}

impl PolicyValueNet {
    /// Forward pass returning `(policy_logits, value)` of shapes
    /// `[batch, output_width]` and `[batch, 1]`.
    pub fn forward(&self, input: Tensor<2>) -> (Tensor<2>, Tensor<2>) {
        let ph = self
            .activation
            .forward(self.policy_hidden.forward(input.clone()));
        let logits = self.policy_head.forward(ph);
        let vh = self.activation.forward(self.value_hidden.forward(input));
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

    fn expand(&self, decision: &Decision<O>, root_noise: bool) -> Node {
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
            edges,
        }
    }

    fn quick_act(&self, decision: &Decision<O>, substream: u64) -> Choice {
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
        let chosen_idx =
            select_by_temperature(&legal_indices, &eval.priors, 0.5, decision.key, substream);
        self.space
            .decode(chosen_idx, &decision.legal)
            .unwrap_or(Choice::Wait(Wait::NextEvent))
    }

    fn rollout<E: Env<Obs = O>>(
        &self,
        sim: &mut E,
        mut dec: Decision<O>,
        mut elapsed: SimDuration,
        sim_idx: u32,
    ) -> (f64, SimDuration, bool) {
        let mut total_reward = 0.0;
        for step in 0..16 {
            if self.config.horizon == SimDuration::ZERO || elapsed >= self.config.horizon {
                return (total_reward, elapsed, false);
            }
            let substream = 100 + u64::from(sim_idx) * 31 + step;
            let choice = self.quick_act(&dec, substream);
            let Ok(tr) = sim.step(choice) else {
                break;
            };
            total_reward += tr.reward;
            elapsed = elapsed + tr.elapsed;
            match tr.next {
                Turn::Done(_) => return (total_reward, elapsed, true),
                Turn::Decide(next_dec) => dec = next_dec,
            }
        }
        (total_reward, elapsed, false)
    }
}

impl<E: Env> SearchPolicy<E> for Mcts<E::Obs> {
    fn search(&mut self, env: &E, decision: &Decision<E::Obs>) -> SearchResult {
        let mut nodes = vec![self.expand(decision, true)];
        let c_puct = f64::from(self.config.c_puct);
        let mut min_q = f64::INFINITY;
        let mut max_q = f64::NEG_INFINITY;

        for sim_idx in 0..self.config.simulations {
            let mut sim = env.clone();
            let mut node_id = 0usize;
            let mut elapsed_total = SimDuration::ZERO;
            let mut path: Vec<(usize, usize, f64)> = Vec::new();

            let leaf_value = loop {
                let node = &nodes[node_id];
                let total_visits: u32 = node.edges.iter().map(|e| e.visits).sum();
                let sqrt_total = f64::from(total_visits.max(1)).sqrt();

                let norm_q = |e: &Edge| -> f64 {
                    if e.visits == 0 {
                        0.5
                    } else {
                        let q = e.value_sum / f64::from(e.visits);
                        if max_q > min_q + 1e-6 {
                            ((q - min_q) / (max_q - min_q)).clamp(0.0, 1.0)
                        } else {
                            0.5
                        }
                    }
                };

                let (edge_pos, _) = node
                    .edges
                    .iter()
                    .enumerate()
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
                    .expect("expanded node has at least one edge");

                let choice = nodes[node_id].edges[edge_pos].choice.clone();
                let Ok(tr) = sim.step(choice) else {
                    break 0.0;
                };
                elapsed_total = elapsed_total + tr.elapsed;
                path.push((node_id, edge_pos, tr.reward));

                match tr.next {
                    Turn::Done(_) => break 0.0,
                    Turn::Decide(next_dec) => {
                        let reached_horizon = self.config.horizon > SimDuration::ZERO
                            && elapsed_total >= self.config.horizon;
                        if let Some(child_id) = nodes[node_id].edges[edge_pos].child {
                            if reached_horizon {
                                let tree_r: f64 = path.iter().map(|(_, _, r)| *r).sum();
                                let h_ms = f64::from(self.config.horizon.millis());
                                let e_ms = f64::from(elapsed_total.millis()).max(1.0);
                                break tree_r * (h_ms / e_ms) - tree_r;
                            }
                            node_id = child_id;
                        } else {
                            let child = self.expand(&next_dec, false);
                            let child_id = nodes.len();
                            nodes.push(child);
                            nodes[node_id].edges[edge_pos].child = Some(child_id);
                            let (roll_r, final_elapsed, done) = if reached_horizon {
                                (0.0, elapsed_total, false)
                            } else {
                                self.rollout(&mut sim, next_dec, elapsed_total, sim_idx)
                            };
                            if !done
                                && self.config.horizon > SimDuration::ZERO
                                && final_elapsed > SimDuration::ZERO
                            {
                                let tree_r: f64 = path.iter().map(|(_, _, r)| *r).sum();
                                let raw_total = tree_r + roll_r;
                                let h_ms = f64::from(self.config.horizon.millis());
                                let e_ms = f64::from(final_elapsed.millis()).max(1.0);
                                break raw_total * (h_ms / e_ms) - tree_r;
                            }
                            break roll_r;
                        }
                    }
                }
            };

            let mut g = leaf_value;
            for (nid, epos, reward) in path.into_iter().rev() {
                g += reward;
                let edge = &mut nodes[nid].edges[epos];
                edge.visits += 1;
                edge.value_sum += g;
                let q = edge.value_sum / f64::from(edge.visits);
                min_q = min_q.min(q);
                max_q = max_q.max(q);
            }
        }

        let root = &nodes[0];
        let total_visits: u32 = root.edges.iter().map(|e| e.visits).sum();
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
                    let w = f64::from(e.visits) * (2.5 * nq).exp();
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
            root.edges
                .iter()
                .max_by(|a, b| {
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
                    a.visits
                        .cmp(&b.visits)
                        .then_with(|| qa.total_cmp(&qb))
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

        SearchResult {
            choice: chosen_edge.choice.clone(),
            visits,
            value,
        }
    }
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
}
