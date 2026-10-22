//! Reinforcement learning loop over a [`Bundle`] using [Burn](https://burn.dev)
//! and PUCT [`Mcts`](portunus_policy::Mcts).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use burn::tensor::Device;
use portunus_core::{Seat, Seed, SimDuration, SpecId};
use portunus_env::{ActionSpace, Decision, DenseReward, Env, SpecActionSpace, Turn};
use portunus_eval::{Estimate, Experiment, LocalRunner, Metric, Runner, SeedSet};
use portunus_export::{
    AddonEmitter, AddonTarget, BurnDistiller, Distiller, LuaAddonEmitter, StudentNetConfig,
};
use portunus_learn::{
    BurnTrainer, Collector, DirStore, MemoryStore, ModelStore, Sample, SelfPlayCollector, Trainer,
};
use portunus_policy::{
    take_mcts_profile_micros, Featurizer, MctsConfig, Model, ModelPolicy, ModelVersion, Policy,
    PolicyValueNetConfig, UniformModel,
};

use crate::arm::{AbilityRow, Breakdown, SimArm};
use crate::bundle::{Bundle, RunError};
use crate::observe::{SeatObs, SeatObsFeaturizer};

/// Hyperparameters and seed configuration for [`train_rl`].
#[derive(Debug, Clone)]
pub struct TrainSpec {
    pub seat: usize,
    pub iterations: usize,
    pub episodes_per_iter: u32,
    pub warm_start_episodes: u32,
    pub eval_seeds: SeedSet,
    pub simulations: u32,
    pub horizon: SimDuration,
    pub hidden_width: usize,
    pub num_layers: usize,
    pub epochs_per_iter: usize,
    pub learning_rate: f64,
    pub save_dir: Option<PathBuf>,
}

impl Default for TrainSpec {
    fn default() -> Self {
        Self {
            seat: 0,
            iterations: 6,
            episodes_per_iter: 3,
            warm_start_episodes: 0,
            eval_seeds: SeedSet {
                first: 1_000,
                count: 10,
            },
            simulations: 96,
            horizon: SimDuration(10_000),
            hidden_width: 64,
            num_layers: 2,
            epochs_per_iter: 30,
            learning_rate: 0.01,
            save_dir: None,
        }
    }
}

/// Metrics and losses after one RL iteration (`iteration == 0` is the
/// untrained network before any self-play).
#[derive(Debug, Clone)]
pub struct IterationSummary {
    pub iteration: usize,
    pub version: ModelVersion,
    pub samples_total: usize,
    pub policy_loss: f32,
    pub value_loss: f32,
    pub dps: Estimate,
    pub total_time: Estimate,
    pub completion_rate: f64,
    pub top_abilities: Vec<AbilityRow>,
}

/// Full report of a multi-iteration Burn RL run against the bundle's
/// scripted baseline.
#[derive(Debug, Clone)]
pub struct TrainReport {
    pub run_name: String,
    pub seat: Seat,
    pub spec_id: SpecId,
    pub spec_name: String,
    pub hero_tree_name: Option<String>,
    pub feature_width: usize,
    pub action_space_size: usize,
    pub hidden_width: usize,
    pub num_layers: usize,
    pub simulations: u32,
    pub horizon: SimDuration,
    pub warm_start_episodes: u32,
    pub eval_seeds: SeedSet,
    pub scripted_baseline_name: String,
    pub scripted_dps: Estimate,
    pub scripted_time: Estimate,
    pub scripted_breakdown: Breakdown,
    pub initial_breakdown: Breakdown,
    pub final_breakdown: Breakdown,
    pub iterations: Vec<IterationSummary>,
    pub distilled_agreement: f64,
    pub addon_files: Vec<(String, usize)>,
    pub mcts_env_secs: f64,
    pub mcts_nn_secs: f64,
    pub train_backprop_secs: f64,
    pub eval_rollout_secs: f64,
}

fn find_metric(metrics: &[(Metric, Estimate)], wanted: Metric) -> Estimate {
    metrics
        .iter()
        .find(|(m, _)| *m == wanted)
        .map(|(_, e)| *e)
        .unwrap_or(Estimate {
            n: 0,
            mean: 0.0,
            std_err: 0.0,
            ci95: (0.0, 0.0),
        })
}

fn eval_policy_metrics(
    bundle: &Bundle,
    runner: &LocalRunner,
    seat: usize,
    name: String,
    policy: Arc<dyn Policy<SeatObs>>,
    seeds: SeedSet,
) -> Result<(Estimate, Estimate, f64), RunError> {
    let seat_id = Seat(u8::try_from(seat).unwrap_or(0));
    let mut arm: SimArm = bundle.arm(false);
    arm.name = name;
    arm.policies[seat] = policy;
    let exp = Experiment {
        arms: vec![Arc::new(arm)],
        baseline: 0,
        seeds,
        metrics: bundle.metrics(),
    };
    let rep = runner.run(&exp);
    let arm_rep = &rep.arms[0];
    let dps = find_metric(&arm_rep.metrics, Metric::SeatDps(seat_id));
    let time = find_metric(&arm_rep.metrics, Metric::TotalTime);
    let comp = find_metric(&arm_rep.metrics, Metric::CompletionRate).mean;
    Ok((dps, time, comp))
}

fn eval_policy_breakdown(
    bundle: &Bundle,
    seat: usize,
    policy: Arc<dyn Policy<SeatObs>>,
    seeds: SeedSet,
) -> Breakdown {
    let mut trace_arm = bundle.arm(true);
    trace_arm.policies[seat] = policy;
    crate::arm::breakdown(&trace_arm, &bundle.data, seeds).unwrap_or(Breakdown {
        runs: 0,
        combat_secs: 0.0,
        rows: Vec::new(),
    })
}

fn eval_policy(
    bundle: &Bundle,
    runner: &LocalRunner,
    seat: usize,
    name: String,
    policy: Arc<dyn Policy<SeatObs>>,
    seeds: SeedSet,
) -> Result<(Estimate, Estimate, f64, Breakdown), RunError> {
    let (dps, time, comp) =
        eval_policy_metrics(bundle, runner, seat, name, Arc::clone(&policy), seeds)?;
    let bd = eval_policy_breakdown(bundle, seat, policy, seeds);
    Ok((dps, time, comp, bd))
}

/// Runs a multi-iteration self-play RL loop using PUCT [`Mcts`](portunus_policy::Mcts)
/// and Burn's [`BurnTrainer`] (`PolicyValueNet` + `AdamW`), evaluating each
/// checkpoint against the bundle's scripted SimC baseline on `spec.eval_seeds`.
pub fn train_rl(
    bundle: &Bundle,
    runner: &LocalRunner,
    spec: TrainSpec,
) -> Result<TrainReport, RunError> {
    if spec.seat >= bundle.templates.len() {
        return Err(RunError::NoSuchSeat(spec.seat));
    }
    let seat_id = Seat(u8::try_from(spec.seat).unwrap_or(0));
    let template = &bundle.templates[spec.seat];
    let spec_id = template.spec;
    let spec_name = bundle
        .data
        .specs
        .get(&spec_id)
        .map_or_else(|| format!("Spec {}", spec_id.0), |s| s.name.clone());
    let hero_tree_name = template
        .hero_tree
        .and_then(|h| bundle.data.hero_trees.get(&h))
        .map(|h| h.name.clone());

    let space_concrete = SpecActionSpace::for_spec(&bundle.data, spec_id).expect("action space");
    let action_space_size = space_concrete.size();
    let space: Arc<dyn ActionSpace> = Arc::new(space_concrete);

    let feat_concrete = SeatObsFeaturizer::for_spec(&bundle.data, spec_id).expect("featurizer");
    let feature_width = feat_concrete.width();
    let schema = feat_concrete.schema.clone();
    let featurizer: Arc<dyn Featurizer<SeatObs>> = Arc::new(feat_concrete);

    let _ = take_mcts_profile_micros();
    let mut train_backprop_secs = 0.0f64;
    let mut eval_rollout_secs = 0.0f64;

    // 1. Evaluate scripted SimC baseline on eval_seeds.
    let scripted_baseline_name = bundle.config.seats[spec.seat].policy.clone();
    let scripted_policy = Arc::clone(&bundle.policies[spec.seat]);
    let t_eval0 = Instant::now();
    let (scripted_dps, scripted_time, _, scripted_breakdown) = eval_policy(
        bundle,
        runner,
        spec.seat,
        scripted_baseline_name.clone(),
        Arc::clone(&scripted_policy),
        spec.eval_seeds,
    )?;
    eval_rollout_secs += t_eval0.elapsed().as_secs_f64();

    // 2. Initialize Burn PolicyValueNet trainer, collector, and checkpoint store.
    let model_name = format!("{}_burn_rl", bundle.config.name);
    let net_cfg = PolicyValueNetConfig::new(feature_width, action_space_size)
        .with_hidden_width(spec.hidden_width)
        .with_num_layers(spec.num_layers.max(1));
    let mut trainer = BurnTrainer::new(
        ModelVersion {
            name: model_name.clone(),
            step: 0,
        },
        schema.clone(),
        net_cfg,
        spec.learning_rate,
        1e-4,
        Device::default(),
    );

    let reward_scale = (10_000.0 / scripted_dps.mean.max(1_000.0)).clamp(0.005, 2.0);
    let reward = Arc::new(DenseReward {
        per_k_damage: reward_scale,
        per_pull_cleared: 50.0,
        per_second: 2.5,
        per_death: 100.0,
    });
    let rl_env = bundle.env_with(crate::observe::ScriptObserver, reward, false);
    let mcts_cfg = MctsConfig {
        simulations: spec.simulations,
        c_puct: 1.25,
        dirichlet_alpha: 0.3,
        dirichlet_frac: 0.20,
        temperature: 0.15,
        horizon: spec.horizon,
    };
    let mut collector = SelfPlayCollector::new(
        rl_env.clone(),
        mcts_cfg,
        Arc::clone(&space),
        Arc::clone(&featurizer),
    );
    collector.discount = 0.85;
    collector.max_steps = 96;
    collector.threads = runner.threads;

    let mut warm_buffer: Vec<Sample> = Vec::new();
    let mut mcts_buffer: Vec<Sample> = Vec::new();
    let mut samples_total = 0usize;
    let mut iter0_stats = TrainStats::default();
    if spec.warm_start_episodes > 0 {
        let warm_seeds: Vec<Seed> = (0..spec.warm_start_episodes)
            .map(|k| Seed(50_000 + u64::from(k)))
            .collect();
        let warm_trajs = collector.collect_with_policy(
            &*scripted_policy,
            ModelVersion {
                name: model_name.clone(),
                step: 0,
            },
            &warm_seeds,
        );
        let warm_samples: Vec<Sample> = warm_trajs.into_iter().flat_map(|t| t.samples).collect();
        let warm_epochs = (spec.epochs_per_iter * 2).max(1);
        let t_tr = Instant::now();
        for _ in 0..warm_epochs {
            iter0_stats = trainer.train(&warm_samples);
        }
        train_backprop_secs += t_tr.elapsed().as_secs_f64();
        samples_total += warm_samples.len();
        warm_buffer = warm_samples;
    }

    let mut mem_store = MemoryStore::new();
    let mut dir_store = spec
        .save_dir
        .as_ref()
        .map(|dir| DirStore::new(dir).expect("create checkpoint dir"));
    let ckpt0 = trainer.checkpoint();
    mem_store.save(&ckpt0).expect("save initial checkpoint");
    if let Some(ds) = &mut dir_store {
        let _ = ds.save(&ckpt0);
    }

    // 3. Evaluate Iteration 0 (untrained or warm-started BurnModel).
    let m0 = mem_store.load(&ckpt0.version).expect("load m0");
    let policy0: Arc<dyn Policy<SeatObs>> = Arc::new(ModelPolicy {
        space: Arc::clone(&space),
        featurizer: Arc::clone(&featurizer),
        model: Arc::clone(&m0),
        temperature: 0.0,
    });
    let t_ev = Instant::now();
    let (dps0, time0, comp0, initial_breakdown) = eval_policy(
        bundle,
        runner,
        spec.seat,
        "burn_iter_0".into(),
        policy0,
        spec.eval_seeds,
    )?;
    eval_rollout_secs += t_ev.elapsed().as_secs_f64();
    eprintln!(
        "[train] baseline={:.1} dps | iter 0={:.1} dps ({} samples)",
        scripted_dps.mean, dps0.mean, samples_total
    );

    let mut iterations = Vec::with_capacity(spec.iterations + 1);
    iterations.push(IterationSummary {
        iteration: 0,
        version: ckpt0.version,
        samples_total,
        policy_loss: iter0_stats.policy_loss,
        value_loss: iter0_stats.value_loss,
        dps: dps0,
        total_time: time0,
        completion_rate: comp0,
        top_abilities: initial_breakdown.rows.iter().take(6).cloned().collect(),
    });

    // 4. Self-play + Burn AdamW hill-climb training loop.
    let mut final_breakdown = initial_breakdown.clone();
    let mut best_dps = dps0.mean;
    let mut best_net = trainer.net.clone();
    let mut best_model = m0;

    for iter in 1..=spec.iterations {
        let search_prior_model: Arc<dyn Model> = if iter == 1 && spec.warm_start_episodes == 0 {
            Arc::new(UniformModel {
                version: ModelVersion {
                    name: model_name.clone(),
                    step: 0,
                },
                schema: schema.clone(),
                input_width: feature_width,
                output_width: action_space_size,
            })
        } else {
            Arc::clone(&best_model)
        };

        let base_seed =
            u64::try_from(iter - 1).unwrap_or(0) * u64::from(spec.episodes_per_iter) + 1;
        let train_seeds: Vec<Seed> = (0..spec.episodes_per_iter)
            .map(|k| Seed(base_seed + u64::from(k)))
            .collect();

        let trajs = collector.collect(search_prior_model, &train_seeds);
        let new_samples: Vec<Sample> = trajs.into_iter().flat_map(|t| t.samples).collect();
        samples_total += new_samples.len();
        mcts_buffer.extend(new_samples.clone());
        if mcts_buffer.len() > 2_400 {
            let drain = mcts_buffer.len() - 2_400;
            mcts_buffer.drain(0..drain);
        }

        let warm_keep = (warm_buffer.len() / (2 * iter + 1)).min(warm_buffer.len());
        let mut train_batch: Vec<Sample> =
            Vec::with_capacity(warm_keep + mcts_buffer.len() + 2 * new_samples.len());
        train_batch.extend_from_slice(&warm_buffer[..warm_keep]);
        train_batch.extend_from_slice(&mcts_buffer);
        train_batch.extend_from_slice(&new_samples);
        train_batch.extend_from_slice(&new_samples);

        trainer.net = best_net.clone();
        let half_epochs = (spec.epochs_per_iter / 2).max(1);
        let mut stats_a = TrainStats::default();
        let t_tr_a = Instant::now();
        for _ in 0..half_epochs {
            stats_a = trainer.train(&train_batch);
        }
        train_backprop_secs += t_tr_a.elapsed().as_secs_f64();
        let net_a = trainer.net.clone();
        let ckpt_a = trainer.checkpoint();
        mem_store.save(&ckpt_a).expect("save checkpoint a");
        let model_a = mem_store.load(&ckpt_a.version).expect("load model a");
        let policy_a: Arc<dyn Policy<SeatObs>> = Arc::new(ModelPolicy {
            space: Arc::clone(&space),
            featurizer: Arc::clone(&featurizer),
            model: Arc::clone(&model_a),
            temperature: 0.0,
        });
        let t_ev_a = Instant::now();
        let (dps_a, time_a, comp_a) = eval_policy_metrics(
            bundle,
            runner,
            spec.seat,
            format!("burn_iter_{iter}a"),
            Arc::clone(&policy_a),
            spec.eval_seeds,
        )?;
        eval_rollout_secs += t_ev_a.elapsed().as_secs_f64();

        let rem_epochs = (spec.epochs_per_iter - half_epochs).max(1);
        let mut stats_b = stats_a;
        let t_tr_b = Instant::now();
        for _ in 0..rem_epochs {
            stats_b = trainer.train(&new_samples);
        }
        train_backprop_secs += t_tr_b.elapsed().as_secs_f64();
        let net_b = trainer.net.clone();
        let ckpt_b = trainer.checkpoint();
        mem_store.save(&ckpt_b).expect("save checkpoint b");
        let model_b = mem_store.load(&ckpt_b.version).expect("load model b");
        let policy_b: Arc<dyn Policy<SeatObs>> = Arc::new(ModelPolicy {
            space: Arc::clone(&space),
            featurizer: Arc::clone(&featurizer),
            model: Arc::clone(&model_b),
            temperature: 0.0,
        });
        let t_ev_b = Instant::now();
        let (dps_b, time_b, comp_b) = eval_policy_metrics(
            bundle,
            runner,
            spec.seat,
            format!("burn_iter_{iter}b"),
            Arc::clone(&policy_b),
            spec.eval_seeds,
        )?;
        eval_rollout_secs += t_ev_b.elapsed().as_secs_f64();

        let (stats, ckpt, latest_model, cand_net, dps, time, comp, win_policy) =
            if dps_b.mean >= dps_a.mean {
                (
                    stats_b, ckpt_b, model_b, net_b, dps_b, time_b, comp_b, policy_b,
                )
            } else {
                (
                    stats_a, ckpt_a, model_a, net_a, dps_a, time_a, comp_a, policy_a,
                )
            };
        let t_bd = Instant::now();
        let bd = eval_policy_breakdown(bundle, spec.seat, win_policy, spec.eval_seeds);
        eval_rollout_secs += t_bd.elapsed().as_secs_f64();

        eprintln!(
            "[train] iter {iter}/{}: {:.1} dps ({:.1}% of baseline, {:.1}s, {} samples)",
            spec.iterations,
            dps.mean,
            100.0 * dps.mean / scripted_dps.mean.max(1.0),
            time.mean,
            samples_total
        );
        if dps.mean >= best_dps {
            best_dps = dps.mean;
            best_net = cand_net;
            best_model = Arc::clone(&latest_model);
            final_breakdown = bd.clone();
            if let Some(ds) = &mut dir_store {
                let _ = ds.save(&ckpt);
            }
        } else {
            trainer.net = best_net.clone();
            trainer.learning_rate = (trainer.learning_rate * 0.85).max(0.0015);
        }

        iterations.push(IterationSummary {
            iteration: iter,
            version: ckpt.version,
            samples_total,
            policy_loss: stats.policy_loss,
            value_loss: stats.value_loss,
            dps,
            total_time: time,
            completion_rate: comp,
            top_abilities: bd.rows.iter().take(6).cloned().collect(),
        });
    }

    // 5. Distill best trained BurnModel into a compact BurnStudent and emit Lua addon.
    let teacher = ModelPolicy {
        space: Arc::clone(&space),
        featurizer: Arc::clone(&featurizer),
        model: Arc::clone(&best_model),
        temperature: 0.0,
    };
    let mut decisions: Vec<Decision<SeatObs>> = Vec::new();
    let mut dist_env = rl_env;
    for s in 0..spec.eval_seeds.count.min(4) {
        let seed = Seed(spec.eval_seeds.first + u64::from(s));
        if let Ok(mut turn) = dist_env.reset(seed) {
            while let Turn::Decide(d) = turn {
                let choice = teacher.act(&d);
                if d.seat == seat_id {
                    decisions.push(d);
                }
                let Ok(tr) = dist_env.step(choice) else {
                    break;
                };
                turn = tr.next;
            }
        }
    }

    let student_cfg = StudentNetConfig::new(feature_width, action_space_size).with_hidden_width(32);
    let distiller = BurnDistiller::new(student_cfg, Arc::clone(&space), Arc::clone(&featurizer));
    let student = distiller.distill(&teacher, &decisions);
    let distilled_agreement = distiller.agreement(&student, &teacher, &decisions);
    let addon = LuaAddonEmitter.emit(
        &student,
        &AddonTarget {
            addon_name: "PortunusRotation".into(),
            interface: 120100,
        },
    );
    let addon_files = addon
        .files
        .into_iter()
        .map(|(path, content)| (path, content.len()))
        .collect();

    let (mcts_env_us, mcts_nn_us) = take_mcts_profile_micros();
    let mcts_env_secs = mcts_env_us as f64 / 1_000_000.0;
    let mcts_nn_secs = mcts_nn_us as f64 / 1_000_000.0;

    Ok(TrainReport {
        run_name: bundle.config.name.clone(),
        seat: seat_id,
        spec_id,
        spec_name,
        hero_tree_name,
        feature_width,
        action_space_size,
        hidden_width: spec.hidden_width,
        num_layers: spec.num_layers.max(1),
        simulations: spec.simulations,
        horizon: spec.horizon,
        warm_start_episodes: spec.warm_start_episodes,
        eval_seeds: spec.eval_seeds,
        scripted_baseline_name,
        scripted_dps,
        scripted_time,
        scripted_breakdown,
        initial_breakdown,
        final_breakdown,
        iterations,
        distilled_agreement,
        addon_files,
        mcts_env_secs,
        mcts_nn_secs,
        train_backprop_secs,
        eval_rollout_secs,
    })
}

use portunus_learn::TrainStats;
