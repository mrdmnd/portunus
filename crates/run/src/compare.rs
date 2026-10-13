//! Two policies for one seat, on the same seeds.
//!
//! Everything else about the run stays as its file says, so the paired
//! differences are the policies' alone.

use std::sync::Arc;

use portunus_core::Seat;
use portunus_eval::{ArmError, Estimate, Experiment, Metric, Report, Runner, SeedSet};
use thiserror::Error;

use crate::arm::{breakdown, Breakdown};
use crate::bundle::{Bundle, RunError};

#[derive(Debug, Error)]
pub enum CompareError {
    #[error(transparent)]
    Run(#[from] RunError),
    #[error("breakdown: {0}")]
    Breakdown(#[from] ArmError),
}

/// What to compare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareSpec {
    /// Index into the run's seats.
    pub seat: usize,
    pub baseline: String,
    pub candidate: String,
    pub seeds: SeedSet,
    /// Also play both again with traces for per-ability breakdowns.
    pub breakdowns: bool,
}

/// Whether the candidate's paired change clears zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Better,
    Worse,
    Unclear,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub spec: CompareSpec,
    /// Arm 0 is the baseline, arm 1 the candidate.
    pub report: Report,
    /// Baseline's, then the candidate's.
    pub breakdowns: Option<(Breakdown, Breakdown)>,
}

impl Comparison {
    pub fn seat(&self) -> Seat {
        Seat(u8::try_from(self.spec.seat).unwrap_or(u8::MAX))
    }

    /// The candidate's paired change in the seat's DPS.
    pub fn dps_change(&self) -> Option<&Estimate> {
        let metric = Metric::SeatDps(self.seat());
        self.report
            .paired
            .iter()
            .find(|d| d.metric == metric)
            .map(|d| &d.delta)
    }

    /// The baseline's mean DPS for the seat.
    pub fn baseline_dps(&self) -> Option<f64> {
        let metric = Metric::SeatDps(self.seat());
        self.report
            .arms
            .first()?
            .metrics
            .iter()
            .find(|(m, _)| *m == metric)
            .map(|(_, e)| e.mean)
    }

    /// Better or worse when the 95% interval of the paired DPS change
    /// excludes zero.
    pub fn verdict(&self) -> Verdict {
        match self.dps_change() {
            Some(d) if d.n > 1 && d.ci95.0 > 0.0 => Verdict::Better,
            Some(d) if d.n > 1 && d.ci95.1 < 0.0 => Verdict::Worse,
            _ => Verdict::Unclear,
        }
    }
}

/// Plays the baseline and the candidate in `spec.seat` on the same seeds.
pub fn compare(
    bundle: &Bundle,
    runner: &impl Runner,
    spec: CompareSpec,
) -> Result<Comparison, CompareError> {
    let arms = |record_trace: bool| -> Result<_, RunError> {
        let base = bundle.arm_with(record_trace, spec.seat, &spec.baseline)?;
        let mut cand = bundle.arm_with(record_trace, spec.seat, &spec.candidate)?;
        if cand.name == base.name {
            cand.name = format!("{} (again)", cand.name);
        }
        Ok((base, cand))
    };
    let (base, cand) = arms(false)?;
    let report = runner.run(&Experiment {
        arms: vec![Arc::new(base), Arc::new(cand)],
        baseline: 0,
        seeds: spec.seeds,
        metrics: bundle.metrics(),
    });
    let breakdowns = if spec.breakdowns {
        let (base, cand) = arms(true)?;
        Some((
            breakdown(&base, &bundle.data, spec.seeds)?,
            breakdown(&cand, &bundle.data, spec.seeds)?,
        ))
    } else {
        None
    };
    Ok(Comparison {
        spec,
        report,
        breakdowns,
    })
}
