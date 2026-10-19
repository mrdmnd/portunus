//! Export to a rotation-helper addon using [Burn](https://burn.dev).
//!
//! A full search tree or large network can't run in the game's Lua at useful
//! speed, so export is two steps: distill the teacher into a compact Burn
//! student network ([`StudentNet`] / [`BurnStudent`]) via [`BurnDistiller`],
//! then emit Lua ([`LuaAddonEmitter`]) that evaluates the student's weights.
//! Only students of `InfoSet::Realistic` observations are exportable, since
//! the addon can't see anything more. Because observations are a function of
//! current state only, the addon needs no history: it reads the game state at
//! each decision and nothing else.

use std::fmt::Write as _;
use std::sync::Arc;

use burn::nn::{Linear, LinearConfig, Relu};
use burn::optim::{AdamWConfig, GradientsParams, LearningRate};
use burn::prelude::*;
use burn::tensor::activation::log_softmax;
use portunus_engine::{Choice, Wait};
use portunus_env::{ActionSpace, Decision};
use portunus_policy::{Featurizer, Policy};

pub trait Distiller<O> {
    type Student;
    /// Fit a student to the teacher's choices on these decisions.
    fn distill(&self, teacher: &dyn Policy<O>, decisions: &[Decision<O>]) -> Self::Student;
    /// Share of decisions where the student agrees with the teacher.
    fn agreement(
        &self,
        student: &Self::Student,
        teacher: &dyn Policy<O>,
        decisions: &[Decision<O>],
    ) -> f64;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonTarget {
    pub addon_name: String,
    /// The game's interface version, e.g. `110205`.
    pub interface: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonBundle {
    /// `(relative path, contents)`.
    pub files: Vec<(String, String)>,
}

pub trait AddonEmitter<S> {
    fn emit(&self, student: &S, target: &AddonTarget) -> AddonBundle;
}

/// Configuration for the compact Burn [`StudentNet`].
#[derive(Config, Debug, PartialEq, Eq)]
pub struct StudentNetConfig {
    pub input_width: usize,
    #[config(default = 32)]
    pub hidden_width: usize,
    pub output_width: usize,
}

/// Compact two-layer Burn [`Module`] used for policy distillation and Lua
/// addon emission.
#[derive(Module, Debug)]
pub struct StudentNet {
    pub hidden: Linear,
    pub activation: Relu,
    pub output: Linear,
}

impl StudentNetConfig {
    pub fn init(&self, device: &Device) -> StudentNet {
        StudentNet {
            hidden: LinearConfig::new(self.input_width, self.hidden_width).init(device),
            activation: Relu::new(),
            output: LinearConfig::new(self.hidden_width, self.output_width).init(device),
        }
    }
}

impl StudentNet {
    pub fn forward(&self, input: Tensor<2>) -> Tensor<2> {
        let h = self.activation.forward(self.hidden.forward(input));
        self.output.forward(h)
    }
}

/// Distilled student policy backed by a Burn [`StudentNet`].
#[derive(Clone)]
pub struct BurnStudent<O> {
    pub config: StudentNetConfig,
    pub space: Arc<dyn ActionSpace>,
    pub featurizer: Arc<dyn Featurizer<O>>,
    pub device: Device,
    pub net: StudentNet,
}

impl<O> BurnStudent<O> {
    pub fn predict_index(&self, decision: &Decision<O>) -> Option<usize> {
        let in_w = self.config.input_width;
        let out_w = self.config.output_width;
        let mut features = self.featurizer.featurize(&decision.obs, &decision.legal);
        features.resize(in_w, 0.0);
        let x = Tensor::<2>::from_data(TensorData::new(features, [1, in_w]), &self.device);
        let logits: Vec<f32> = self
            .net
            .forward(x)
            .into_data()
            .try_to_vec()
            .unwrap_or_else(|_| vec![0.0; out_w]);
        let legal = self.space.legal(&decision.legal);
        (0..out_w)
            .filter(|&a| *legal.get(a).unwrap_or(&false))
            .max_by(|&a, &b| logits[a].total_cmp(&logits[b]).then_with(|| b.cmp(&a)))
    }
}

impl<O> Policy<O> for BurnStudent<O> {
    fn act(&self, decision: &Decision<O>) -> Choice {
        self.predict_index(decision)
            .and_then(|idx| self.space.decode(idx, &decision.legal))
            .unwrap_or(Choice::Wait(Wait::NextEvent))
    }
}

/// Burn-powered [`Distiller`] that trains a [`StudentNet`] with `AdamW` to
/// imitate a teacher [`Policy`].
pub struct BurnDistiller<O> {
    pub config: StudentNetConfig,
    pub space: Arc<dyn ActionSpace>,
    pub featurizer: Arc<dyn Featurizer<O>>,
    pub epochs: usize,
    pub learning_rate: LearningRate,
    pub weight_decay: f32,
}

impl<O> BurnDistiller<O> {
    pub fn new(
        config: StudentNetConfig,
        space: Arc<dyn ActionSpace>,
        featurizer: Arc<dyn Featurizer<O>>,
    ) -> Self {
        Self {
            config,
            space,
            featurizer,
            epochs: 40,
            learning_rate: 0.05,
            weight_decay: 1e-4,
        }
    }
}

impl<O> Distiller<O> for BurnDistiller<O> {
    type Student = BurnStudent<O>;

    fn distill(&self, teacher: &dyn Policy<O>, decisions: &[Decision<O>]) -> Self::Student {
        let in_w = self.config.input_width;
        let out_w = self.config.output_width;
        let ad_device = Device::default().autodiff();
        let mut net = self.config.init(&ad_device);
        let mut optim = AdamWConfig::new()
            .with_weight_decay(self.weight_decay)
            .init();

        let mut flat_x = Vec::new();
        let mut flat_target = Vec::new();
        let mut flat_illegal = Vec::new();
        let mut count = 0usize;

        for d in decisions {
            let choice = teacher.act(d);
            let Some(target_idx) = self.space.encode(&choice, &d.legal) else {
                continue;
            };
            if target_idx >= out_w {
                continue;
            }
            let mut feat = self.featurizer.featurize(&d.obs, &d.legal);
            feat.resize(in_w, 0.0);
            let legal = self.space.legal(&d.legal);
            flat_x.extend_from_slice(&feat);
            for a in 0..out_w {
                let is_legal = *legal.get(a).unwrap_or(&false) || a == target_idx;
                flat_illegal.push(!is_legal);
                flat_target.push(if a == target_idx { 1.0f32 } else { 0.0 });
            }
            count += 1;
        }

        if count > 0 {
            let x = Tensor::<2>::from_data(TensorData::new(flat_x, [count, in_w]), &ad_device);
            let target_pi =
                Tensor::<2>::from_data(TensorData::new(flat_target, [count, out_w]), &ad_device);
            let illegal_mask = Tensor::<2, Bool>::from_data(
                TensorData::new(flat_illegal, [count, out_w]),
                &ad_device,
            );

            for _ in 0..self.epochs {
                let logits = net.forward(x.clone());
                let masked = logits.mask_fill(illegal_mask.clone(), -1e9);
                let log_probs = log_softmax(masked, 1);
                let loss = -(target_pi.clone() * log_probs).sum_dim(1).mean();
                let grads = GradientsParams::from_grads(loss.backward(), &net);
                net = optim.step(self.learning_rate, net, grads);
            }
        }

        BurnStudent {
            config: self.config.clone(),
            space: Arc::clone(&self.space),
            featurizer: Arc::clone(&self.featurizer),
            device: Device::default(),
            net: net.valid(),
        }
    }

    fn agreement(
        &self,
        student: &Self::Student,
        teacher: &dyn Policy<O>,
        decisions: &[Decision<O>],
    ) -> f64 {
        if decisions.is_empty() {
            return 1.0;
        }
        let matches = decisions
            .iter()
            .filter(|d| student.act(d) == teacher.act(d))
            .count();
        matches as f64 / decisions.len() as f64
    }
}

/// Emits a World of Warcraft `.toc` and `.lua` addon bundle that evaluates a
/// distilled [`BurnStudent`]'s Burn layer weights.
#[derive(Debug, Clone, Copy, Default)]
pub struct LuaAddonEmitter;

impl<O> AddonEmitter<BurnStudent<O>> for LuaAddonEmitter {
    fn emit(&self, student: &BurnStudent<O>, target: &AddonTarget) -> AddonBundle {
        let toc_path = format!("{}/{}.toc", target.addon_name, target.addon_name);
        let lua_path = format!("{}/model.lua", target.addon_name);

        let toc = format!(
            "## Interface: {}\n## Title: {}\n## Notes: Distilled Portunus Burn policy\nmodel.lua\n",
            target.interface, target.addon_name
        );

        let w1: Vec<f32> = student
            .net
            .hidden
            .weight
            .val()
            .into_data()
            .try_to_vec()
            .unwrap_or_default();
        let b1: Vec<f32> = student
            .net
            .hidden
            .bias
            .as_ref()
            .and_then(|b| b.val().into_data().try_to_vec().ok())
            .unwrap_or_else(|| vec![0.0; student.config.hidden_width]);
        let w2: Vec<f32> = student
            .net
            .output
            .weight
            .val()
            .into_data()
            .try_to_vec()
            .unwrap_or_default();
        let b2: Vec<f32> = student
            .net
            .output
            .bias
            .as_ref()
            .and_then(|b| b.val().into_data().try_to_vec().ok())
            .unwrap_or_else(|| vec![0.0; student.config.output_width]);

        let mut lua = String::new();
        let _ = writeln!(lua, "-- Generated by Portunus (Burn StudentNet)");
        let _ = writeln!(lua, "local M = {{}}");
        let _ = writeln!(lua, "M.input_width = {}", student.config.input_width);
        let _ = writeln!(lua, "M.hidden_width = {}", student.config.hidden_width);
        let _ = writeln!(lua, "M.output_width = {}", student.config.output_width);
        write_lua_array(&mut lua, "M.w1", &w1);
        write_lua_array(&mut lua, "M.b1", &b1);
        write_lua_array(&mut lua, "M.w2", &w2);
        write_lua_array(&mut lua, "M.b2", &b2);
        let _ = writeln!(
            lua,
            "function M.forward(x)\n  local h = {{}}\n  for j = 1, M.hidden_width do\n    local s = M.b1[j]\n    for i = 1, M.input_width do\n      s = s + (x[i] or 0) * M.w1[(i - 1) * M.hidden_width + j]\n    end\n    h[j] = s > 0 and s or 0\n  end\n  local out = {{}}\n  for k = 1, M.output_width do\n    local s = M.b2[k]\n    for j = 1, M.hidden_width do\n      s = s + h[j] * M.w2[(j - 1) * M.output_width + k]\n    end\n    out[k] = s\n  end\n  return out\nend\nreturn M"
        );

        AddonBundle {
            files: vec![(toc_path, toc), (lua_path, lua)],
        }
    }
}

fn write_lua_array(out: &mut String, name: &str, values: &[f32]) {
    let _ = write!(out, "{name} = {{");
    for (i, v) in values.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{v:.6}");
    }
    out.push_str("}\n");
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use portunus_core::{Seat, Seed, SpellId};
    use portunus_engine::{ActionMask, Readiness, WakeReason};
    use portunus_env::{DecisionKey, SpecActionSpace};
    use portunus_policy::{IdentityFeaturizer, SchemaId};

    use super::*;

    const SPELL_A: SpellId = SpellId(100);
    const SPELL_B: SpellId = SpellId(200);

    #[test]
    fn burn_distiller_trains_student_to_match_teacher_and_emits_lua_addon() {
        let space: Arc<dyn ActionSpace> =
            Arc::new(SpecActionSpace::from_spells([SPELL_A, SPELL_B], 0, 0));
        let featurizer: Arc<dyn Featurizer<Vec<f32>>> = Arc::new(IdentityFeaturizer {
            schema: SchemaId("export_test:v1".into()),
            width: 2,
        });
        let teacher = |d: &Decision<Vec<f32>>| {
            if d.obs[0] > d.obs[1] {
                Choice::cast(SPELL_A)
            } else {
                Choice::cast(SPELL_B)
            }
        };

        let mask = ActionMask {
            abilities: BTreeMap::from([(SPELL_A, Readiness::Now), (SPELL_B, Readiness::Now)]),
            can_stop: false,
            can_wait_tick: false,
            targets: Vec::new(),
            cancel: Vec::new(),
            can_move: false,
            can_stop_move: false,
        };
        let make_decision = |idx: u64, x0: f32, x1: f32| Decision {
            seat: Seat(0),
            reason: WakeReason::CombatStart,
            obs: vec![x0, x1],
            legal: mask.clone(),
            key: DecisionKey {
                seed: Seed(1),
                seat: Seat(0),
                index: idx,
            },
        };

        let decisions = vec![
            make_decision(0, 1.0, 0.0),
            make_decision(1, 0.9, 0.1),
            make_decision(2, 0.0, 1.0),
            make_decision(3, 0.1, 0.9),
        ];

        let cfg = StudentNetConfig::new(2, space.size()).with_hidden_width(16);
        let distiller = BurnDistiller::new(cfg, space, featurizer);
        let student = distiller.distill(&teacher, &decisions);
        let acc = distiller.agreement(&student, &teacher, &decisions);
        assert!(acc >= 0.99, "expected full agreement, got {acc}");

        let bundle = LuaAddonEmitter.emit(
            &student,
            &AddonTarget {
                addon_name: "PortunusHelper".into(),
                interface: 120100,
            },
        );
        assert_eq!(bundle.files.len(), 2);
        assert!(bundle.files[0].0.ends_with("PortunusHelper.toc"));
        assert!(bundle.files[1].1.contains("function M.forward(x)"));
    }
}
