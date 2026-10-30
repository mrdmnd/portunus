//! Plain-text reports.

use std::collections::BTreeMap;
use std::fmt::Write;

use portunus_core::{PetId, SpellId};
use portunus_eval::{Estimate, Metric, Report};

use crate::arm::{AbilityRow, Breakdown};
use crate::compare::{Comparison, Verdict};
use crate::opt::OptimizeReport;
use crate::parity::SimcParityReport;
use crate::train::TrainReport;

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

/// Renders a multi-iteration Burn RL training report (`train_rl`).
pub fn render_training(r: &TrainReport) -> String {
    let mut out = String::new();
    let hero = r.hero_tree_name.as_deref().unwrap_or("None");
    let _ = writeln!(
        out,
        "burn rl training: {} (seat {}, {} [{}], {} features -> {}x{} residual -> {} actions)",
        r.run_name,
        r.seat.0,
        r.spec_name,
        hero,
        r.feature_width,
        r.num_layers,
        r.hidden_width,
        r.action_space_size
    );
    let _ = writeln!(
        out,
        "  mcts: {} simulations, {:.1}s lookahead horizon, warm-start episodes={} | eval seeds {}..{} (n={})",
        r.simulations,
        f64::from(r.horizon.millis()) / 1000.0,
        r.warm_start_episodes,
        r.eval_seeds.first,
        r.eval_seeds.first + u64::from(r.eval_seeds.count),
        r.eval_seeds.count
    );
    let _ = writeln!(
        out,
        "  profile: mcts_env={:.2}s (thread-sum), mcts_nn={:.2}s (thread-sum), burn_backprop={:.2}s (wall), eval_rollout={:.2}s (wall)",
        r.mcts_env_secs,
        r.mcts_nn_secs,
        r.train_backprop_secs,
        r.eval_rollout_secs
    );
    let _ = writeln!(
        out,
        "  scripted baseline ({}): {:>10.1} dps ± {:<7.1}  time {:>6.2}s",
        r.scripted_baseline_name,
        r.scripted_dps.mean,
        1.96 * r.scripted_dps.std_err,
        r.scripted_time.mean,
    );
    let _ = writeln!(
        out,
        "\n  {:>4} {:>8} {:>8} {:>10} {:>10} {:>12} {:>10} {:>9}",
        "iter", "step", "samples", "pol_loss", "val_loss", "dps", "vs_script", "time(s)"
    );
    for it in &r.iterations {
        let pct_vs_script = if r.scripted_dps.mean > 0.0 {
            100.0 * it.dps.mean / r.scripted_dps.mean
        } else {
            0.0
        };
        let _ = writeln!(
            out,
            "  {:>4} {:>8} {:>8} {:>10.4} {:>10.2} {:>12.1} {:>8.1}% {:>9.2}",
            it.iteration,
            it.version.step,
            it.samples_total,
            it.policy_loss,
            it.value_loss,
            it.dps.mean,
            pct_vs_script,
            it.total_time.mean,
        );
    }
    let _ = writeln!(out, "\ntrained policy vs untrained (iter 0 -> final):");
    out += &render_breakdown_change(&r.initial_breakdown, &r.final_breakdown);
    let _ = writeln!(
        out,
        "\ntrained policy vs scripted baseline ({} -> final):",
        r.scripted_baseline_name
    );
    out += &render_breakdown_change(&r.scripted_breakdown, &r.final_breakdown);
    out
}

/// Renders a pre-run secondary-stat racing optimization report (`optimize_stats`).
pub fn render_optimization(r: &OptimizeReport) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "racing stat optimization: {} (seat {}, {})",
        r.run_name, r.seat.0, r.spec_name
    );
    let _ = writeln!(
        out,
        "  start: crit={} ({:.1}%) haste={} ({:.1}%) mast={} ({:.1}%) vers={} ({:.1}%) -> {:.1} dps (n={})",
        r.start_point.ratings[0],
        r.start_derived.crit_pct,
        r.start_point.ratings[1],
        r.start_derived.haste_pct,
        r.start_point.ratings[2],
        r.start_derived.mastery_pct,
        r.start_point.ratings[3],
        r.start_derived.versatility_pct,
        r.start_dps.mean,
        r.start_dps.n
    );
    let gain = r.best_dps.mean - r.start_dps.mean;
    let pct = if r.start_dps.mean > 0.0 {
        100.0 * gain / r.start_dps.mean
    } else {
        0.0
    };
    let _ = writeln!(
        out,
        "  best:  crit={} ({:.1}%) haste={} ({:.1}%) mast={} ({:.1}%) vers={} ({:.1}%) -> {:.1} dps ({:+.1} dps, {:+.2}%, n={})",
        r.best_point.ratings[0],
        r.best_derived.crit_pct,
        r.best_point.ratings[1],
        r.best_derived.haste_pct,
        r.best_point.ratings[2],
        r.best_derived.mastery_pct,
        r.best_point.ratings[3],
        r.best_derived.versatility_pct,
        r.best_dps.mean,
        gain,
        pct,
        r.best_dps.n
    );
    let _ = writeln!(
        out,
        "\n  {:>4} {:<44} {:>8} {:>12} {:>10} {:>6}",
        "id", "allocation", "feasible", "dps", "±95% ci", "seeds"
    );
    for (id, desc, feas, est) in r.leaderboard.iter().take(12) {
        let _ = writeln!(
            out,
            "  {:>4} {:<44} {:>8} {:>12.1} {:>10.1} {:>6}",
            id.0,
            desc,
            if *feas { "yes" } else { "no" },
            est.mean,
            1.96 * est.std_err,
            est.n
        );
    }
    out
}

/// Renders a side-by-side Patchwerk parity comparison against SimulationCraft.
pub fn render_parity(r: &SimcParityReport) -> String {
    let mut out = String::new();
    let hero = r.hero_tree_name.as_deref().unwrap_or("None");
    let _ = writeln!(
        out,
        "simc patchwerk parity: {} (spec {} {} [{}])",
        r.run_name, r.spec_id.0, r.spec_name, hero
    );
    let _ = writeln!(
        out,
        "  simc profile: {} | dump diffs applied: {}",
        r.simc_profile.display(),
        r.dump_diffs_applied
    );
    if let Some(t) = &r.talents_override {
        let _ = writeln!(out, "  simc ran this loadout's hero tree: talents={t}");
    }
    if !r.unmapped_talents.is_empty() {
        let _ = writeln!(
            out,
            "  simc talents missing here: {}",
            r.unmapped_talents.join(", ")
        );
    }
    if !r.unmodelled_gear.is_empty() {
        let _ = writeln!(
            out,
            "  simc gear effects missing here: {}",
            r.unmodelled_gear.join(", ")
        );
    }
    let _ = writeln!(
        out,
        "  portunus dps: {:>10.1} ± {:<7.1} ({:.1}s combat, n={})",
        r.portunus_dps.mean,
        1.96 * r.portunus_dps.std_err,
        r.portunus_combat_secs,
        r.portunus_dps.n
    );
    let gate_ok = r.within_tolerance(crate::parity::DEFAULT_PARITY_TOLERANCE);
    let gate_str = if gate_ok { "PASS" } else { "FAIL" };
    let _ = writeln!(
        out,
        "  simc dps:     {:>10.1}           (ratio = {:.1}%, delta = {:+.1}%, ≤0.5% gate = {})",
        r.simc_dps,
        100.0 * r.dps_ratio,
        100.0 * (r.dps_ratio - 1.0),
        gate_str
    );
    let cast_divs = r.cast_rate_divergences(0.35);
    if !cast_divs.is_empty() {
        let names: Vec<String> = cast_divs
            .iter()
            .map(|d| {
                format!(
                    "{} ({:.1} vs {:.1})",
                    d.name, d.portunus_casts, d.simc_casts
                )
            })
            .collect();
        let _ = writeln!(out, "  cast-rate divergences (>35%): {}", names.join(", "));
    }
    let _ = writeln!(
        out,
        "\n  {:<28} {:>8} {:>8} {:>11} {:>11} {:>9} {:>8} {:>8}",
        "ability", "p_casts", "s_casts", "portunus", "simc", "dps_diff", "p_share", "s_share"
    );
    for row in &r.rows {
        let _ = writeln!(
            out,
            "  {:<28} {:>8.1} {:>8.1} {:>11.1} {:>11.1} {:>+9.1} {:>7.1}% {:>7.1}%",
            row.name,
            row.portunus_casts,
            row.simc_casts,
            row.portunus_dps,
            row.simc_dps,
            row.portunus_dps - row.simc_dps,
            100.0 * row.portunus_share,
            100.0 * row.simc_share
        );
    }
    out
}
