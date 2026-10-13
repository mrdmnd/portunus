//! Plain-text reports.

use std::collections::BTreeMap;
use std::fmt::Write;

use portunus_core::{PetId, SpellId};
use portunus_eval::{Estimate, Metric, Report};

use crate::arm::{AbilityRow, Breakdown};
use crate::compare::{Comparison, Verdict};

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

/// Both arms' metrics and paired changes, the verdict on the seat's DPS,
/// and, with breakdowns, each ability's change, largest first.
pub fn render_comparison(c: &Comparison) -> String {
    let s = &c.spec;
    let seeds = s.seeds;
    let mut out = format!(
        "{} against {}, seat {}, seeds {}..{} ({})\n",
        s.candidate,
        s.baseline,
        s.seat,
        seeds.first,
        seeds.first + u64::from(seeds.count),
        seeds.count
    );
    out += &render(&c.report, None);
    if let Some(d) = c.dps_change() {
        let pct = |v: f64| match c.baseline_dps() {
            Some(b) if b > 0.0 => format!("{:+.2}%", 100.0 * v / b),
            _ => "n/a".into(),
        };
        let verdict = match c.verdict() {
            Verdict::Better => "better",
            Verdict::Worse => "worse",
            Verdict::Unclear => "no clear difference",
        };
        let _ = writeln!(
            out,
            "\nseat {} dps change: {:+.2} ({}), 95% [{:+.2}, {:+.2}] ({} to {}): {verdict}",
            s.seat,
            d.mean,
            pct(d.mean),
            d.ci95.0,
            d.ci95.1,
            pct(d.ci95.0),
            pct(d.ci95.1),
        );
    }
    if let Some((base, cand)) = &c.breakdowns {
        out += &render_breakdown_change(base, cand);
    }
    out
}

type RowKey = (Option<PetId>, Option<SpellId>);

fn render_breakdown_change(base: &Breakdown, cand: &Breakdown) -> String {
    let mut rows: BTreeMap<RowKey, (Option<&AbilityRow>, Option<&AbilityRow>)> = BTreeMap::new();
    for r in &base.rows {
        rows.entry((r.pet, r.spell)).or_default().0 = Some(r);
    }
    for r in &cand.rows {
        rows.entry((r.pet, r.spell)).or_default().1 = Some(r);
    }
    let field = |r: Option<&AbilityRow>, f: fn(&AbilityRow) -> f64| r.map_or(0.0, f);
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        let change = |(x, y): &(Option<&AbilityRow>, Option<&AbilityRow>)| {
            (field(*y, |r| r.dps) - field(*x, |r| r.dps)).abs()
        };
        change(b).total_cmp(&change(a))
    });
    let mut out = format!(
        "\nby ability, per run ({:.1} s in combat before, {:.1} s after; share change in points)\n",
        base.combat_secs, cand.combat_secs
    );
    let _ = writeln!(
        out,
        "  {:<28} {:>8} {:>8} {:>8} {:>10} {:>10} {:>9} {:>8}",
        "ability", "casts", "after", "change", "dps", "after", "change", "share"
    );
    for (b, c) in rows {
        let Some(name) = c.or(b).map(|r| r.name.as_str()) else {
            continue;
        };
        let (casts, casts2) = (field(b, |r| r.casts), field(c, |r| r.casts));
        let (dps, dps2) = (field(b, |r| r.dps), field(c, |r| r.dps));
        let share = 100.0 * (field(c, |r| r.share) - field(b, |r| r.share));
        let _ = writeln!(
            out,
            "  {name:<28} {casts:>8.1} {casts2:>8.1} {:>+8.1} {dps:>10.1} {dps2:>10.1} {:>+9.1} {share:>+7.1}%",
            casts2 - casts,
            dps2 - dps,
        );
    }
    out
}
