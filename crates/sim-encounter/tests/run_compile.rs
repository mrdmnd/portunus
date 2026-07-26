//! Run-definition compiler tests: schema shape, validation, sampling
//! determinism, distribution bounds, and name-keyed CRN stability.

use std::path::Path;

use sim_encounter::{bestiary, RunError, RunSampler, RunSpec, ScenarioSampler};
use sim_types::{
    from_millis, EngageSpec, ResolvedCombat, RunSegment, ScriptedEventKind, Seed, TriggerExpr,
};

fn fixture(rel: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(rel)
}

fn sampler() -> RunSampler {
    let spec = RunSpec::from_path(&fixture("runs/toy_hall.run.toml")).expect("run parses");
    let beasts =
        bestiary::load_path(&fixture("toy_codex/bestiary.toml")).expect("bestiary parses");
    RunSampler::new(spec, beasts).expect("fixture validates")
}

fn combat<'r>(run: &'r sim_types::ResolvedRun, name: &str) -> &'r ResolvedCombat {
    run.combats().find(|c| c.name == name).expect("combat present")
}

#[test]
fn fixture_compiles_with_expected_shape() {
    let run = sampler().sample(Seed(7));

    // travel, opener, travel, overseer.
    assert_eq!(run.segments.len(), 4);
    assert!(matches!(run.segments[0], RunSegment::Travel { .. }));
    assert!(matches!(run.segments[2], RunSegment::Travel { .. }));

    let opener = combat(&run, "opener");
    // 2 grunts + 1 patrol + 1 ritualist, labeled deterministically.
    let labels: Vec<&str> = opener.spawns.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["grunt#1", "grunt#2", "patrol#1", "ritualist#1"]);
    assert!(matches!(opener.spawns[0].engage, EngageSpec::AtStart));
    assert_eq!(opener.spawns[2].engage, EngageSpec::After(from_millis(8000)));
    assert_eq!(
        opener.spawns[3].engage,
        EngageSpec::When(TriggerExpr::EngagedHpFracBelow(0.4))
    );

    // Trash-onto-boss: one combat, boss at start, grunts timed in.
    let boss = combat(&run, "overseer");
    let labels: Vec<&str> = boss.spawns.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels, ["overseer#1", "grunt#1", "grunt#2"]);
    assert!(matches!(boss.spawns[0].engage, EngageSpec::AtStart));
    assert_eq!(boss.spawns[1].engage, EngageSpec::After(from_millis(5000)));

    // Scripts are unrolled, engagement-relative, sorted, within the timeout.
    let overseer = &boss.spawns[0];
    assert!(!overseer.script.is_empty());
    assert!(overseer.script.windows(2).all(|w| w[0].offset <= w[1].offset));
    assert!(overseer.script.iter().all(|o| o.offset <= boss.timeout));
}

#[test]
fn same_seed_is_bit_identical_and_seeds_differ() {
    let s = sampler();
    let a = s.sample(Seed(42));
    let b = s.sample(Seed(42));
    assert_eq!(a, b, "same seed => identical ResolvedRun (invariant 1)");

    let c = s.sample(Seed(43));
    assert_ne!(a, c, "different seed => different rolls");
}

#[test]
fn sampled_values_respect_declared_distributions() {
    let s = sampler();
    for seed in 0..50_u64 {
        let run = s.sample(Seed(seed));

        // opener travel_in: 8000 ± 1000 ms.
        let RunSegment::Travel { duration } = run.segments[0] else {
            panic!("expected leading travel")
        };
        assert!((from_millis(7000)..=from_millis(9000)).contains(&duration));

        let opener = combat(&run, "opener");
        // grunt hp: 15000 ± 10%.
        let g = &opener.spawns[0];
        assert!((13_500.0..=16_500.0).contains(&g.max_health), "hp {}", g.max_health);

        // grunt swipe: first 3000±500, gaps 5000±1000.
        let offsets: Vec<_> = g.script.iter().map(|o| o.offset).collect();
        assert!((from_millis(2500)..=from_millis(3500)).contains(&offsets[0]));
        for w in offsets.windows(2) {
            let gap = w[1] - w[0];
            assert!(
                (from_millis(4000)..=from_millis(6000)).contains(&gap),
                "gap {gap} out of bounds"
            );
        }

        // Intake targets were resolved at sample time (no in-engine RNG).
        assert!(g
            .script
            .iter()
            .all(|o| matches!(o.kind, ScriptedEventKind::Intake { target, .. }
                if matches!(target, sim_types::IntakeTarget::Player(_)))));
    }
}

/// RNG domains are keyed on names, not sequence position: editing one pull
/// must not perturb another pull's rolls (CRN pairing survives spec edits).
#[test]
fn name_keyed_domains_survive_spec_edits() {
    let s = sampler();
    let before = s.sample(Seed(11));

    // Drop the overseer pull entirely; the opener must roll identically.
    let mut spec = s.spec().clone();
    spec.pulls.retain(|p| p.name == "opener");
    spec.count_requirement = None;
    let beasts =
        bestiary::load_path(&fixture("toy_codex/bestiary.toml")).expect("bestiary parses");
    let edited = RunSampler::new(spec, beasts).expect("edited spec validates");
    let after = edited.sample(Seed(11));

    assert_eq!(
        combat(&before, "opener"),
        combat(&after, "opener"),
        "opener rolls must not depend on other pulls existing"
    );
}

#[test]
fn validation_rejects_bad_specs() {
    let beasts =
        bestiary::load_path(&fixture("toy_codex/bestiary.toml")).expect("bestiary parses");

    let bad = r#"
        name = "bad"
        count_requirement = 999

        [[pulls]]
        name = "p1"
        [[pulls.engage]]
        mobs = { gruntt = 1 }

        [[pulls.engage]]
        mobs = { grunt = 1 }
        after_ms = 1000
        when = { engaged_hp_frac_below = 0.5 }

        [[pulls.engage]]
        mobs = { grunt = 1 }
        when = { spawn_dead = "nosuch#1" }

        [[pulls.cooldowns]]
        player = 99
        ability = "x"
        anchor = "grunt#1/nosuchscript@1"
    "#;
    let spec = RunSpec::from_toml_str(bad).expect("syntactically valid TOML");
    let err = RunSampler::new(spec, beasts).expect_err("must fail validation");
    let RunError::Invalid(msgs) = err;
    let all = msgs.join("\n");
    assert!(all.contains("'gruntt' not in bestiary"), "unknown enemy: {all}");
    assert!(all.contains("both after_ms and when"), "exclusive engage: {all}");
    assert!(all.contains("spawn_dead 'nosuch#1'"), "dangling spawn ref: {all}");
    assert!(all.contains("player 99 out of party range"), "player range: {all}");
    assert!(all.contains("no script 'nosuchscript'"), "dangling script ref: {all}");
    assert!(all.contains("count requirement not met"), "count ledger: {all}");
}

#[test]
fn plan_compiles_static_commitments() {
    let s = sampler();
    let plan = s.plan();

    let opener = plan.pull("opener").expect("opener plan");
    assert!(opener.lust);
    assert_eq!(opener.windows.len(), 2);
    assert!(matches!(opener.windows[0].anchor, sim_types::WindowAnchor::PullStart));

    let boss = plan.pull("overseer").expect("overseer plan");
    assert!(!boss.lust);
    assert!(matches!(
        boss.windows[0].anchor,
        sim_types::WindowAnchor::Offset(t) if t == from_millis(5000)
    ));
    assert!(matches!(
        &boss.windows[1].anchor,
        sim_types::WindowAnchor::ScriptEvent { spawn, script, occurrence: 2 }
            if spawn == "overseer#1" && script == "slam"
    ));
}

#[test]
fn priors_expose_mean_schedules() {
    let priors = sampler().priors();
    assert!(priors
        .event_means
        .iter()
        .any(|(k, t)| k == "overseer/overseer#1/slam" && *t == from_millis(12_000)));
}
