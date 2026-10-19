//! `portunus-run <run.ron> [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]`
//!
//! `portunus-run compare <run.ron> --policy NAME [--baseline NAME] [--seat K]
//! [--holdout] [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]`
//! plays seat `K` (default 0) with `NAME` against the run file's own policy
//! (or `--baseline`) on the same seeds. `--holdout` uses the run's held-out
//! seeds instead of its tuning seeds.
//!
//! `portunus-run train <run.ron> [--iterations N] [--episodes E] [--simulations S]
//! [--epochs K] [--seat K] [--seeds N] [--first-seed S] [--threads T]`
//! runs a multi-iteration Burn self-play RL loop (`Mcts` + `BurnTrainer` + `BurnDistiller`)
//! and compares each checkpoint against the bundle's scripted baseline.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use portunus_eval::{LocalRunner, Runner, SeedSet};
use portunus_run::{compare, report, train_rl, Bundle, CompareSpec, TrainSpec};

#[derive(Default)]
struct Args {
    compare: bool,
    train: bool,
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
    simulations: Option<u32>,
    horizon_ms: Option<u32>,
    epochs: Option<usize>,
    save_dir: Option<PathBuf>,
}

const USAGE: &str =
    "usage: portunus-run <run.ron> [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]
       portunus-run compare <run.ron> --policy NAME [--baseline NAME] [--seat K] [--holdout]
                    [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]
       portunus-run train <run.ron> [--iterations N] [--episodes E] [--simulations S]
                    [--horizon-ms MS] [--epochs K] [--seat K] [--save-dir DIR]
                    [--seeds N] [--first-seed S] [--threads T]";

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
            "--seat" if args.compare || args.train => {
                args.seat = number("--seat", value("--seat")?)?;
            }
            "--holdout" if args.compare || args.train => args.holdout = true,
            "--iterations" if args.train => {
                args.iterations = Some(number("--iterations", value("--iterations")?)?);
            }
            "--episodes" if args.train => {
                args.episodes = Some(number("--episodes", value("--episodes")?)?);
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
    let bundle = match Bundle::load(&args.path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
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
