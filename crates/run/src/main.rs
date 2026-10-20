//! `portunus-run <run.ron> [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]`
//!
//! `portunus-run compare <run.ron> --policy NAME [--baseline NAME] [--seat K]
//! [--holdout] [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]`
//! plays seat `K` (default 0) with `NAME` against the run file's own policy
//! (or `--baseline`) on the same seeds. `--holdout` uses the run's held-out
//! seeds instead of its tuning seeds.
//!
//! `portunus-run train <run.ron> [--iterations N] [--episodes E] [--warm-start W]
//! [--simulations S] [--horizon-ms MS] [--epochs K] [--seat K] [--seeds N] [--first-seed S] [--threads T]`
//! runs a multi-iteration Burn self-play RL loop (`Mcts` + `BurnTrainer` + `BurnDistiller`)
//! and compares each checkpoint against the bundle's scripted baseline.
//!
//! `portunus-run optimize <run.ron> [--rounds R] [--step-rating S] [--seat K]
//! [--seeds N] [--first-seed S] [--threads T]`
//! runs adaptive seed-racing (`RacingOptimizer`) over secondary-stat allocations.
//!
//! `portunus-run parity <run.ron> [--simc-bin PATH] [--simc-profile PATH] [--dump-dir DIR]
//! [--no-sync-dump] [--seeds N] [--first-seed S] [--threads T]`
//! syncs spell coefficients from `SpellDataDump` and runs a side-by-side Patchwerk
//! comparison against the local `simc` CLI binary.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use portunus_eval::{LocalRunner, Runner, SeedSet};
use portunus_optimize::RacingConfig;
use portunus_run::{
    check_simc_parity, compare, default_simc_profile, optimize_stats, report, train_rl, Bundle,
    CompareSpec, TrainSpec,
};

#[derive(Default)]
struct Args {
    compare: bool,
    train: bool,
    optimize: bool,
    parity: bool,
    path: PathBuf,
    seeds: Option<u32>,
    first_seed: Option<u64>,
    threads: Option<NonZeroUsize>,
    no_breakdown: bool,
    policy: Option<String>,
    baseline: Option<String>,
    seat: usize,
    holdout: bool,
    iterations: Option<usize>,
    episodes: Option<u32>,
    warm_start: Option<u32>,
    simulations: Option<u32>,
    horizon_ms: Option<u32>,
    epochs: Option<usize>,
    save_dir: Option<PathBuf>,
    rounds: Option<usize>,
    step_rating: Option<i64>,
    simc_bin: Option<PathBuf>,
    simc_profile: Option<PathBuf>,
    dump_dir: Option<PathBuf>,
    no_sync_dump: bool,
}

const USAGE: &str =
    "usage: portunus-run <run.ron> [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]
       portunus-run compare <run.ron> --policy NAME [--baseline NAME] [--seat K] [--holdout]
                    [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]
       portunus-run train <run.ron> [--iterations N] [--episodes E] [--warm-start W]
                    [--simulations S] [--horizon-ms MS] [--epochs K] [--seat K] [--save-dir DIR]
                    [--seeds N] [--first-seed S] [--threads T]
       portunus-run optimize <run.ron> [--rounds R] [--step-rating S] [--seat K]
                    [--seeds N] [--first-seed S] [--threads T]
       portunus-run parity <run.ron> [--simc-bin PATH] [--simc-profile PATH] [--dump-dir DIR]
                    [--no-sync-dump] [--seeds N] [--first-seed S] [--threads T]";

fn number<T: std::str::FromStr>(name: &str, v: String) -> Result<T, String>
where
    T::Err: std::fmt::Display,
{
    v.parse().map_err(|e| format!("{name}: {e}"))
}

fn parse() -> Result<Args, String> {
    let mut args = Args::default();
    let mut path = None;
    let mut it = std::env::args().skip(1).peekable();
    if it.peek().is_some_and(|a| a == "compare") {
        it.next();
        args.compare = true;
    } else if it.peek().is_some_and(|a| a == "train") {
        it.next();
        args.train = true;
    } else if it.peek().is_some_and(|a| a == "optimize") {
        it.next();
        args.optimize = true;
    } else if it.peek().is_some_and(|a| a == "parity") {
        it.next();
        args.parity = true;
    }
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "--seeds" => args.seeds = Some(number("--seeds", value("--seeds")?)?),
            "--first-seed" => {
                args.first_seed = Some(number("--first-seed", value("--first-seed")?)?);
            }
            "--threads" => args.threads = Some(number("--threads", value("--threads")?)?),
            "--no-breakdown" => args.no_breakdown = true,
            "--policy" if args.compare => args.policy = Some(value("--policy")?),
            "--baseline" if args.compare => args.baseline = Some(value("--baseline")?),
            "--seat" if args.compare || args.train || args.optimize => {
                args.seat = number("--seat", value("--seat")?)?;
            }
            "--holdout" if args.compare || args.train || args.optimize => args.holdout = true,
            "--iterations" if args.train => {
                args.iterations = Some(number("--iterations", value("--iterations")?)?);
            }
            "--episodes" if args.train => {
                args.episodes = Some(number("--episodes", value("--episodes")?)?);
            }
            "--warm-start" if args.train => {
                args.warm_start = Some(number("--warm-start", value("--warm-start")?)?);
            }
            "--simulations" if args.train => {
                args.simulations = Some(number("--simulations", value("--simulations")?)?);
            }
            "--horizon-ms" if args.train => {
                args.horizon_ms = Some(number("--horizon-ms", value("--horizon-ms")?)?);
            }
            "--epochs" if args.train => {
                args.epochs = Some(number("--epochs", value("--epochs")?)?);
            }
            "--save-dir" if args.train => {
                args.save_dir = Some(PathBuf::from(value("--save-dir")?));
            }
            "--rounds" if args.optimize => {
                args.rounds = Some(number("--rounds", value("--rounds")?)?);
            }
            "--step-rating" if args.optimize => {
                args.step_rating = Some(number("--step-rating", value("--step-rating")?)?);
            }
            "--simc-bin" if args.parity => {
                args.simc_bin = Some(PathBuf::from(value("--simc-bin")?));
            }
            "--simc-profile" if args.parity => {
                args.simc_profile = Some(PathBuf::from(value("--simc-profile")?));
            }
            "--dump-dir" => {
                args.dump_dir = Some(PathBuf::from(value("--dump-dir")?));
            }
            "--no-sync-dump" if args.parity => {
                args.no_sync_dump = true;
            }
            "-h" | "--help" => return Err(USAGE.into()),
            _ if a.starts_with('-') => return Err(format!("unknown flag {a}\n{USAGE}")),
            _ if path.is_none() => path = Some(PathBuf::from(a)),
            _ => return Err(format!("unexpected argument {a}\n{USAGE}")),
        }
    }
    args.path = path.ok_or(USAGE)?;
    if args.compare && args.policy.is_none() {
        return Err(format!("compare needs --policy\n{USAGE}"));
    }
    if args.holdout && args.first_seed.is_some() {
        return Err("--holdout and --first-seed both pick the seeds".into());
    }
    Ok(args)
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut bundle = match Bundle::load(&args.path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(dump_dir) = &args.dump_dir {
        if !args.parity {
            if let Err(e) = bundle.sync_with_dump(dump_dir) {
                eprintln!("error syncing dump: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    let defaults = if args.holdout {
        bundle.holdout_seeds()
    } else {
        bundle.seeds()
    };
    let seeds = SeedSet {
        first: args.first_seed.unwrap_or(defaults.first),
        count: args.seeds.unwrap_or(defaults.count),
    };
    let runner = args
        .threads
        .map_or_else(LocalRunner::available, |threads| LocalRunner { threads });
    if args.compare {
        run_compare(&bundle, &runner, args, seeds)
    } else if args.train {
        run_train(&bundle, &runner, args)
    } else if args.optimize {
        run_optimize(&bundle, &runner, args)
    } else if args.parity {
        run_parity(bundle, &runner, args, seeds)
    } else {
        run_one(&bundle, &runner, &args, seeds)
    }
}

fn run_one(bundle: &Bundle, runner: &LocalRunner, args: &Args, seeds: SeedSet) -> ExitCode {
    let report = runner.run(&bundle.experiment(seeds));
    let breakdown = if args.no_breakdown {
        None
    } else {
        match bundle.breakdown(seeds) {
            Ok(b) => Some(b),
            Err(e) => {
                eprintln!("breakdown failed: {e}");
                None
            }
        }
    };
    print!("{}", report::render(&report, breakdown.as_ref()));
    if report.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run_compare(bundle: &Bundle, runner: &LocalRunner, args: Args, seeds: SeedSet) -> ExitCode {
    let Some(seat) = bundle.config.seats.get(args.seat) else {
        eprintln!("error: no seat {}", args.seat);
        return ExitCode::FAILURE;
    };
    let spec = CompareSpec {
        seat: args.seat,
        baseline: args.baseline.unwrap_or_else(|| seat.policy.clone()),
        candidate: args.policy.unwrap_or_default(),
        seeds,
        breakdowns: !args.no_breakdown,
    };
    match compare(bundle, runner, spec) {
        Ok(c) => {
            print!("{}", report::render_comparison(&c));
            if c.report.failures.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_train(bundle: &Bundle, runner: &LocalRunner, args: Args) -> ExitCode {
    let mut spec = TrainSpec {
        seat: args.seat,
        ..TrainSpec::default()
    };
    if let Some(it) = args.iterations {
        spec.iterations = it;
    }
    if let Some(ep) = args.episodes {
        spec.episodes_per_iter = ep;
    }
    if let Some(ws) = args.warm_start {
        spec.warm_start_episodes = ws;
    }
    if let Some(sims) = args.simulations {
        spec.simulations = sims;
    }
    if let Some(ms) = args.horizon_ms {
        spec.horizon = portunus_core::SimDuration(ms);
    }
    if let Some(epochs) = args.epochs {
        spec.epochs_per_iter = epochs;
    }
    if let Some(first) = args.first_seed {
        spec.eval_seeds.first = first;
    }
    if let Some(count) = args.seeds {
        spec.eval_seeds.count = count;
    }
    spec.save_dir = args.save_dir;
    match train_rl(bundle, runner, spec) {
        Ok(rep) => {
            print!("{}", report::render_training(&rep));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_optimize(bundle: &Bundle, runner: &LocalRunner, args: Args) -> ExitCode {
    let cfg = RacingConfig {
        first_seed: args.first_seed.unwrap_or(0),
        initial_seeds: args.seeds.unwrap_or(4),
        rung_multiplier: 2,
        rungs_per_round: 2,
        max_rounds: args.rounds.unwrap_or(3),
    };
    let step = args.step_rating.unwrap_or(0);
    match optimize_stats(bundle, runner, args.seat, cfg, step) {
        Ok(rep) => {
            print!("{}", report::render_optimization(&rep));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_parity(mut bundle: Bundle, runner: &LocalRunner, args: Args, seeds: SeedSet) -> ExitCode {
    let simc_root = PathBuf::from("target/simc");
    let simc_bin = args
        .simc_bin
        .unwrap_or_else(|| simc_root.join("engine/simc"));
    let dump_dir = args
        .dump_dir
        .unwrap_or_else(|| simc_root.join("SpellDataDump"));

    let dump_diffs_applied = if !args.no_sync_dump && dump_dir.exists() {
        match bundle.sync_with_dump(&dump_dir) {
            Ok(diffs) => diffs.len(),
            Err(e) => {
                eprintln!("warning: dump sync failed: {e}");
                0
            }
        }
    } else {
        0
    };

    let tpl = &bundle.templates[0];
    let Some(simc_profile) = args
        .simc_profile
        .or_else(|| default_simc_profile(tpl.spec, tpl.hero_tree, &simc_root))
    else {
        eprintln!(
            "error: no default SimC profile found for spec {:?}",
            tpl.spec
        );
        return ExitCode::FAILURE;
    };

    let parity_seeds = SeedSet {
        first: seeds.first,
        count: seeds.count.min(30),
    };

    match check_simc_parity(
        &bundle,
        runner,
        &simc_bin,
        &simc_profile,
        parity_seeds,
        dump_diffs_applied,
    ) {
        Ok(rep) => {
            print!("{}", report::render_parity(&rep));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
