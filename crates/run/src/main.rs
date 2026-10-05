//! `portunus-run <run.ron> [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]`

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use portunus_eval::{LocalRunner, Runner, SeedSet};
use portunus_run::{report, Bundle};

struct Args {
    path: PathBuf,
    seeds: Option<u32>,
    first_seed: Option<u64>,
    threads: Option<NonZeroUsize>,
    breakdown: bool,
}

const USAGE: &str =
    "usage: portunus-run <run.ron> [--seeds N] [--first-seed S] [--threads T] [--no-breakdown]";

fn parse() -> Result<Args, String> {
    let mut path = None;
    let mut args = Args {
        path: PathBuf::new(),
        seeds: None,
        first_seed: None,
        threads: None,
        breakdown: true,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "--seeds" => {
                args.seeds = Some(
                    value("--seeds")?
                        .parse()
                        .map_err(|e| format!("--seeds: {e}"))?,
                )
            }
            "--first-seed" => {
                args.first_seed = Some(
                    value("--first-seed")?
                        .parse()
                        .map_err(|e| format!("--first-seed: {e}"))?,
                );
            }
            "--threads" => {
                args.threads = Some(
                    value("--threads")?
                        .parse()
                        .map_err(|e| format!("--threads: {e}"))?,
                );
            }
            "--no-breakdown" => args.breakdown = false,
            "-h" | "--help" => return Err(USAGE.into()),
            _ if a.starts_with('-') => return Err(format!("unknown flag {a}\n{USAGE}")),
            _ if path.is_none() => path = Some(PathBuf::from(a)),
            _ => return Err(format!("unexpected argument {a}\n{USAGE}")),
        }
    }
    args.path = path.ok_or(USAGE)?;
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
    let defaults = bundle.seeds();
    let seeds = SeedSet {
        first: args.first_seed.unwrap_or(defaults.first),
        count: args.seeds.unwrap_or(defaults.count),
    };
    let runner = args
        .threads
        .map_or_else(LocalRunner::available, |threads| LocalRunner { threads });
    let report = runner.run(&bundle.experiment(seeds));
    let breakdown = if args.breakdown {
        match bundle.breakdown(seeds) {
            Ok(b) => Some(b),
            Err(e) => {
                eprintln!("breakdown failed: {e}");
                None
            }
        }
    } else {
        None
    };
    print!("{}", report::render(&report, breakdown.as_ref()));
    if report.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
