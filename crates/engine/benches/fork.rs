//! Fork and rollout cost: `cargo bench -p portunus-engine --bench fork`.

#[path = "../tests/common/mod.rs"]
mod common;

use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;

use portunus_engine::trace::TraceEvent;
use portunus_engine::Engine;

fn main() {
    let f = common::fixture();

    let mut k = common::kernel(&f, 1, 1, common::human());
    let outcome = common::play(&mut k);
    let trace = k.drain_trace();
    let mut casts: BTreeMap<u32, usize> = BTreeMap::new();
    for r in &trace {
        if let TraceEvent::CastStart { spell, .. } = r.event {
            *casts.entry(spell.0).or_default() += 1;
        }
    }
    let ticks = trace
        .iter()
        .filter(|r| matches!(&r.event, TraceEvent::Damage(d) if d.spell.is_none()))
        .count();
    println!(
        "rollout: completed {}, ended at {:.1} s, {} trace events, {ticks} ticks, casts by spell {casts:?}",
        outcome.completed,
        f64::from(outcome.end_time.millis()) / 1000.0,
        trace.len(),
    );

    let mut mid = common::kernel(&f, 1, 1, common::human());
    common::play_for(&mut mid, 60);
    let n = 200_000;
    let t = Instant::now();
    for _ in 0..n {
        black_box(black_box(&mid).clone());
    }
    let per_clone = t.elapsed().as_nanos() as f64 / f64::from(n);
    println!("clone mid-fight: {per_clone:.0} ns");

    let runs = 200;
    let t = Instant::now();
    for seed in 0..runs {
        let mut k = common::kernel(&f, seed, 1, common::human());
        black_box(common::play(&mut k));
    }
    let per_run = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
    println!("full rollout (trace recorded): {per_run:.3} ms");
}
