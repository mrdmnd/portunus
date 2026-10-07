//! Plain-text reports.

use std::fmt::Write;

use portunus_eval::{Estimate, Metric, Report};

use crate::arm::Breakdown;

fn label(m: &Metric) -> String {
    match m {
        Metric::TotalTime => "total time (s)".into(),
        Metric::CompletionRate => "completion rate".into(),
        Metric::Deaths => "deaths".into(),
        Metric::DemandsFailed => "demands failed".into(),
        Metric::DamageTaken => "damage taken".into(),
        Metric::SeatDamage(s) => format!("seat {} damage", s.0),
        Metric::SeatDps(s) => format!("seat {} dps", s.0),
        Metric::DeathChance(p) => format!("{} death chance", p.0),
    }
}

fn est(e: &Estimate) -> String {
    format!(
        "{:>12.2} ± {:<10.2} [{:.2}, {:.2}]  n={}",
        e.mean,
        1.96 * e.std_err,
        e.ci95.0,
        e.ci95.1,
        e.n
    )
}

pub fn render(report: &Report, breakdown: Option<&Breakdown>) -> String {
    let mut out = String::new();
    for arm in &report.arms {
        let _ = writeln!(out, "{}", arm.name);
        for (m, e) in &arm.metrics {
            let _ = writeln!(out, "  {:<18} {}", label(m), est(e));
        }
    }
    for d in &report.paired {
        let _ = writeln!(
            out,
            "  {} vs baseline, {:<18} {}",
            d.arm,
            label(&d.metric),
            est(&d.delta)
        );
    }
    if !report.failures.is_empty() {
        let _ = writeln!(out, "failures:");
        for (arm, seed) in &report.failures {
            let _ = writeln!(out, "  {arm} seed {}", seed.0);
        }
    }
    if let Some(b) = breakdown {
        let _ = writeln!(
            out,
            "\nby ability ({} runs, {:.1} s in combat per run)",
            b.runs, b.combat_secs
        );
        let _ = writeln!(
            out,
            "  {:<28} {:>8} {:>8} {:>7} {:>12} {:>10} {:>7}",
            "ability", "casts", "hits", "crit", "damage", "dps", "share"
        );
        for r in &b.rows {
            let _ = writeln!(
                out,
                "  {:<28} {:>8.1} {:>8.1} {:>6.1}% {:>12.0} {:>10.1} {:>6.1}%",
                r.name,
                r.casts,
                r.hits,
                100.0 * r.crit_rate,
                r.damage,
                r.dps,
                100.0 * r.share
            );
        }
    }
    out
}
