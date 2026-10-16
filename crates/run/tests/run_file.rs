//! The checked-in run files, end to end.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{
    ActorId, AuraId, EnemyKey, EventName, HeroTreeId, PetId, Seat, Seed, SimDuration, SimTime,
    SpellId, Trigger,
};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    BloodlustMode, CastOpts, Choice, MoveGoal, Readiness, StateView, TargetSel, TraceRecord, Wait,
    WakeReason,
};
use portunus_env::{Decision, Env, EpisodeSource, ProgressMeter, Turn};
use portunus_eval::{Arm, LocalRunner, Metric, Runner, SeedSet};
use portunus_gamedata::aura::{
    AuraDef, AuraValue, AuraValueKind, DispelType, GroundDef, RefreshRule,
};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget};
use portunus_gamedata::enemy::{EnemyAction, EnemyKind, EnemyRule};
use portunus_gamedata::spell::{CastKind, Targeting};
use portunus_gamedata::stats::{
    Cost, RechargeDef, ResourceDef, ResourceKind, SchoolMask, SpendScaling,
};
use portunus_run::scripts::buff_top_ally;
use portunus_run::{
    compare, report, BattleResObs, Bundle, ChannelObs, CompareSpec, Comparison, EmpowerObs,
    GroundObs, PartyMeter, RunError, SeatObs, Verdict, FOREVER,
};

const LIGHTNING_BOLT: SpellId = SpellId(188196);

fn bundle() -> Bundle {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs/elemental_dummy.ron");
    Bundle::load(&path).unwrap()
}

#[test]
fn experiment_runs_clean() {
    let b = bundle();
    let seeds = SeedSet {
        first: 0,
        count: 16,
    };
    let report = LocalRunner {
        threads: NonZeroUsize::new(4).unwrap(),
    }
    .run(&b.experiment(seeds));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let metric = |m: Metric| {
        report.arms[0]
            .metrics
            .iter()
            .find(|(x, _)| *x == m)
            .unwrap()
            .1
    };
    assert_eq!(metric(Metric::CompletionRate).mean, 1.0);
    assert!(metric(Metric::SeatDps(portunus_core::Seat(0))).mean > 0.0);
}

#[test]
fn breakdown_accounts_for_all_damage() {
    let b = bundle();
    let seeds = SeedSet { first: 0, count: 4 };
    let bd = b.breakdown(seeds).unwrap();
    let arm = b.arm(false);
    let mean_damage: f64 = seeds
        .iter()
        .map(|s| arm.rollout(s).unwrap().seats[0].damage_done as f64)
        .sum::<f64>()
        / 4.0;
    let total: f64 = bd.rows.iter().map(|r| r.damage).sum();
    assert!((total - mean_damage).abs() < 1e-6 * mean_damage);
    let share: f64 = bd.rows.iter().map(|r| r.share).sum();
    assert!((share - 1.0).abs() < 1e-9);
    assert!(bd
        .rows
        .iter()
        .any(|r| r.name == "Lava Burst" && r.crit_rate > 0.9));
}

#[test]
fn rollouts_repeat_and_forks_agree() {
    let b = bundle();
    let arm = b.arm(false);
    assert_eq!(arm.rollout(Seed(9)).unwrap(), arm.rollout(Seed(9)).unwrap());

    let policy = &arm.policies[0];
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(9)).unwrap();
    for _ in 0..40 {
        let Turn::Decide(d) = turn else {
            panic!("ended early")
        };
        turn = env.step(policy.act(&d)).unwrap().next;
    }
    let finish = |mut env: portunus_run::SimEnv<_>, mut turn| loop {
        match turn {
            Turn::Done(o) => return o,
            Turn::Decide(d) => turn = env.step(policy.act(&d)).unwrap().next,
        }
    };
    let fork = finish(env.clone(), turn.clone());
    let main = finish(env, turn);
    assert_eq!(fork, main);
    assert_eq!(main, arm.rollout(Seed(9)).unwrap());
}

/// A hero tree's run file: forks agree, and policies see the tree.
fn hero_forks_agree(run: &str, tree: HeroTreeId) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/runs")
        .join(run);
    let b = Bundle::load(&path).unwrap();
    let arm = b.arm(false);
    let policy = &arm.policies[0];
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(3)).unwrap();
    for _ in 0..20 {
        let Turn::Decide(d) = turn else {
            panic!("ended early")
        };
        assert_eq!(d.obs.hero_tree, Some(tree));
        turn = env.step(policy.act(&d)).unwrap().next;
    }
    let finish = |mut env: portunus_run::SimEnv<_>, mut turn| loop {
        match turn {
            Turn::Done(o) => return o,
            Turn::Decide(d) => turn = env.step(policy.act(&d)).unwrap().next,
        }
    };
    let fork = finish(env.clone(), turn.clone());
    let main = finish(env, turn);
    assert_eq!(fork, main);
    assert_eq!(main, arm.rollout(Seed(3)).unwrap());
}

#[test]
fn stormbringer_forks_agree_and_policies_see_the_tree() {
    hero_forks_agree("elemental_stormbringer_dummy.ron", HeroTreeId(54));
}

#[test]
fn farseer_forks_agree_and_policies_see_the_tree() {
    hero_forks_agree("elemental_farseer_dummy.ron", HeroTreeId(55));
}

/// The Ancestors' damage is the seat's, and the breakdown credits it to them.
#[test]
fn breakdown_credits_the_ancestors() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/runs/elemental_farseer_dummy.ron");
    let b = Bundle::load(&path).unwrap();
    let seeds = SeedSet { first: 0, count: 4 };
    let bd = b.breakdown(seeds).unwrap();
    let arm = b.arm(false);
    let mean_damage: f64 = seeds
        .iter()
        .map(|s| arm.rollout(s).unwrap().seats[0].damage_done as f64)
        .sum::<f64>()
        / 4.0;
    let total: f64 = bd.rows.iter().map(|r| r.damage).sum();
    assert!((total - mean_damage).abs() < 1e-6 * mean_damage);
    let ancestors = bd
        .rows
        .iter()
        .find(|r| r.pet == Some(PetId(221177)))
        .expect("an Ancestor row");
    assert_eq!(ancestors.name, "Ancestor: Lava Burst");
    assert!(ancestors.casts > 0.0 && ancestors.crit_rate == 1.0);
}

/// The full builds' rotations press every button they talent, and their
/// Elementals come out primal.
#[test]
fn full_builds_press_their_whole_kit() {
    let runs: [(&str, &[&str]); 2] = [
        (
            "elemental_stormbringer_dummy",
            &[
                "Lightning Shield",
                "Stormkeeper",
                "Ascendance",
                "Voltaic Blaze",
                "Lava Burst",
                "Earth Shock",
                "Lightning Bolt",
                "Tempest",
            ],
        ),
        (
            "elemental_farseer_dummy",
            &[
                "Lightning Shield",
                "Stormkeeper",
                "Ancestral Swiftness",
                "Ascendance",
                "Voltaic Blaze",
                "Lava Burst",
                "Elemental Blast",
                "Lightning Bolt",
            ],
        ),
    ];
    for (run, presses) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in presses {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.pet.is_none() && r.name == *name && r.casts > 0.0),
                "{run} casts {name}"
            );
        }
        assert!(
            bd.rows
                .iter()
                .any(|r| r.name == "Primal Fire Elemental: Meteor"),
            "{run}: Call of Fire's Elemental"
        );
    }
}

#[test]
fn arms_policy_uses_the_kit() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs/arms_long.ron");
    let bd = Bundle::load(&path)
        .unwrap()
        .breakdown(SeedSet { first: 0, count: 2 })
        .unwrap();
    let casts = |name: &str| {
        bd.rows
            .iter()
            .filter(|r| r.name == name)
            .map(|r| r.casts)
            .sum::<f64>()
    };
    for name in [
        "Charge",
        "Colossus Smash",
        "Avatar",
        "Bladestorm",
        "Mortal Strike",
        "Overpower",
        "Execute",
        "Heroic Strike",
        "Slam",
        "Cleave",
    ] {
        assert!(casts(name) > 0.0, "casts {name}");
    }
    for name in [
        "Slayer's Strike",
        "Reap the Storm",
        "Mortal Strike (Unhinged)",
    ] {
        assert!(
            bd.rows.iter().any(|r| r.name == name && r.damage > 0.0),
            "{name}"
        );
    }
    // Cleave keeps Rend up, so Rend itself is never pressed.
    assert!(bd.rows.iter().any(|r| r.name == "Rend" && r.damage > 0.0));
    assert_eq!(casts("Rend"), 0.0);
}

#[test]
fn arms_colossus_and_fury_run_files_work() {
    for (run, tree) in [
        ("arms_colossus_dummy.ron", HeroTreeId(62)),
        ("fury_dummy.ron", HeroTreeId(60)),
        ("fury_mountain_thane_dummy.ron", HeroTreeId(61)),
        ("enhancement_stormbringer_dummy.ron", HeroTreeId(54)),
        ("enhancement_totemic_dummy.ron", HeroTreeId(56)),
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/runs")
            .join(run);
        let mut b = Bundle::load(&path).unwrap();
        for e in Arc::make_mut(&mut b.enemies).enemies.values_mut() {
            e.health = 5_000_000;
        }
        let arm = b.arm(false);
        let policy = &arm.policies[0];
        let mut env = b.env(false);
        let mut turn = env.reset(Seed(3)).unwrap();
        for _ in 0..8 {
            let Turn::Decide(d) = turn else {
                panic!("ended early in {run}")
            };
            assert_eq!(d.obs.hero_tree, Some(tree));
            turn = env.step(policy.act(&d)).unwrap().next;
        }
        let finish = |mut env: portunus_run::SimEnv<_>, mut turn| loop {
            match turn {
                Turn::Done(o) => return o,
                Turn::Decide(d) => turn = env.step(policy.act(&d)).unwrap().next,
            }
        };
        let fork = finish(env.clone(), turn.clone());
        let main = finish(env, turn);
        assert_eq!(fork, main);
        assert_eq!(main, arm.rollout(Seed(3)).unwrap());
    }
}

fn arms_bundle() -> Bundle {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs/arms_dummy.ron");
    Bundle::load(&path).unwrap()
}

fn compare_arms(candidate: &str, breakdowns: bool) -> Comparison {
    compare_on_arms("arms", candidate, breakdowns)
}

fn compare_on_arms(baseline: &str, candidate: &str, breakdowns: bool) -> Comparison {
    let b = arms_bundle();
    let spec = CompareSpec {
        seat: 0,
        baseline: baseline.into(),
        candidate: candidate.into(),
        seeds: SeedSet { first: 0, count: 6 },
        breakdowns,
    };
    let runner = LocalRunner {
        threads: NonZeroUsize::new(3).unwrap(),
    };
    compare(&b, &runner, spec).unwrap()
}

#[test]
fn a_policy_against_itself_changes_nothing() {
    let c = compare_arms("arms", true);
    assert!(c.report.failures.is_empty());
    assert_eq!(c.report.arms[1].name, "arms (again)");
    let d = c.dps_change().unwrap();
    assert_eq!((d.n, d.mean, d.std_err), (6, 0.0, 0.0));
    assert_eq!(c.verdict(), Verdict::Unclear);
    assert!(c.baseline_dps().unwrap() > 0.0);
    let (base, cand) = c.breakdowns.as_ref().unwrap();
    assert_eq!(base, cand);
    let text = report::render_comparison(&c);
    assert!(text.contains("no clear difference"), "{text}");
}

#[test]
fn holding_bladestorm_for_imminent_demise_loses() {
    // Three stacks never come in the dummy's few seconds.
    let c = compare_arms("arms_demise3", false);
    assert!(c.report.failures.is_empty());
    assert_eq!(c.verdict(), Verdict::Worse);
    assert!(c.dps_change().unwrap().ci95.1 < -0.05 * c.baseline_dps().unwrap());
}

#[test]
fn compare_tells_a_worse_policy_apart() {
    let c = compare_arms("idle", true);
    assert_eq!(c.verdict(), Verdict::Worse);
    assert!(c.dps_change().unwrap().ci95.1 < 0.0);
    let (base, cand) = c.breakdowns.as_ref().unwrap();
    let casts = |b: &portunus_run::Breakdown| {
        b.rows
            .iter()
            .filter(|r| r.name == "Mortal Strike")
            .map(|r| r.casts)
            .sum::<f64>()
    };
    assert!(casts(base) > 0.0);
    assert_eq!(casts(cand), 0.0);
    let text = report::render_comparison(&c);
    assert!(
        text.contains("worse") && text.contains("Mortal Strike"),
        "{text}"
    );
    // Without breakdowns, only the experiment runs.
    assert!(compare_arms("idle", false).breakdowns.is_none());
    let better = compare_on_arms("idle", "arms", false);
    assert_eq!(better.verdict(), Verdict::Better);
    assert!(report::render_comparison(&better).contains("better"));
}

#[test]
fn compare_names_what_it_cannot_find() {
    let b = arms_bundle();
    assert!(matches!(
        b.arm_with(false, 0, "no_such_policy"),
        Err(RunError::UnknownPolicy { seat: 0, .. })
    ));
    assert!(matches!(
        b.arm_with(false, 3, "arms"),
        Err(RunError::NoSuchSeat(3))
    ));
    assert_eq!(b.arm_with(false, 0, "idle").unwrap().name, "idle");
}

#[test]
fn held_out_seeds_are_apart_from_the_tuning_seeds() {
    let b = arms_bundle();
    let (tune, hold) = (b.seeds(), b.holdout_seeds());
    assert_eq!(tune.count, hold.count);
    assert!(hold.first >= tune.first + u64::from(tune.count));
    let long = Bundle::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs/arms_long.ron"),
    )
    .unwrap();
    assert_eq!(long.holdout_seeds().first, 1_000_000);
}

/// Lava Surge's buff and Lava Burst's reset stay hidden until perceived.
#[test]
fn realistic_seats_miss_what_they_have_not_perceived() {
    let b = bundle();
    let arm = b.arm(false);
    let policy = &arm.policies[0];
    let mut hidden_seen = 0;
    for seed in 0..20 {
        let mut env = b.env(false);
        let mut turn = env.reset(Seed(seed)).unwrap();
        while let Turn::Decide(d) = turn {
            let state = env.state().unwrap();
            for p in state.unperceived(d.seat) {
                match p.reason {
                    WakeReason::AuraGained(a) => {
                        assert!(!d.obs.buffs.contains_key(&a));
                        hidden_seen += 1;
                    }
                    WakeReason::CooldownReady(s) => {
                        assert_ne!(d.legal.abilities.get(&s), Some(&Readiness::Now));
                        hidden_seen += 1;
                    }
                    _ => {}
                }
            }
            turn = env.step(policy.act(&d)).unwrap().next;
        }
    }
    assert!(hidden_seen > 0);
}

/// The script dodges every swirl and quake, uses its movement tools, and
/// chases the dummy back into range after its leap.
#[test]
fn elemental_meets_the_mechanics_dummys_demands() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/runs/elemental_stormbringer_mechanics.ron");
    let b = Bundle::load(&path).unwrap();
    let policy = &b.arm(false).policies[0];
    let mut env = b.env(true);
    let mut turn = env.reset(Seed(2)).unwrap();
    let outcome = loop {
        match turn {
            Turn::Done(o) => break o,
            Turn::Decide(d) => turn = env.step(policy.act(&d)).unwrap().next,
        }
    };
    let trace = env.drain_trace();
    let count = |pick: fn(&TraceEvent) -> bool| trace.iter().filter(|r| pick(&r.event)).count();

    assert!(outcome.completed);
    let placed = count(|e| matches!(e, TraceEvent::Demand { .. }));
    assert!(placed >= 10, "{placed} demands");
    assert_eq!(count(|e| matches!(e, TraceEvent::DemandMet { .. })), placed);
    assert_eq!(outcome.seats[0].demands_failed, 0);
    let far = count(|e| matches!(e, TraceEvent::Distance { distance, .. } if *distance > 40.0));
    assert!(far >= 3, "{far} leaps");
    let chased = trace.iter().any(|r| {
        matches!(
            &r.event,
            TraceEvent::Decision {
                choice: Choice::Move(MoveGoal::Approach { .. }),
                ..
            }
        )
    });
    assert!(chased, "approaches after a leap");
}

/// The shield dummy's barrier soaks a twentieth of its health each time
/// it goes up, and what it soaks is the seat's absorbed damage.
#[test]
fn the_shield_dummys_barriers_soak_its_share_of_health_first() {
    const BARRIER: AuraId = AuraId(910_002);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/runs/elemental_stormbringer_shield.ron");
    let b = Bundle::load(&path).unwrap();
    let policy = &b.arm(false).policies[0];
    let mut env = b.env(true);
    let mut turn = env.reset(Seed(2)).unwrap();
    let outcome = loop {
        match turn {
            Turn::Done(o) => break o,
            Turn::Decide(d) => turn = env.step(policy.act(&d)).unwrap().next,
        }
    };
    let trace = env.drain_trace();
    assert!(outcome.completed);
    // What each barrier soaked, and whether it broke before running out.
    let mut barriers: Vec<(u64, Option<SimDuration>)> = Vec::new();
    let mut raised = None;
    for r in &trace {
        match r.event {
            TraceEvent::AuraApplied { aura, .. } if aura == BARRIER => {
                barriers.push((0, None));
                raised = Some(r.time);
            }
            TraceEvent::Absorbed { aura, amount, .. } if aura == BARRIER => {
                barriers.last_mut().unwrap().0 += amount;
            }
            TraceEvent::AuraRemoved { aura, .. } if aura == BARRIER => {
                barriers.last_mut().unwrap().1 = raised.map(|t| r.time - t);
            }
            _ => {}
        }
    }
    assert!(barriers.len() >= 3, "{barriers:?}");
    assert!(barriers.iter().all(|b| b.0 > 0), "{barriers:?}");
    let total: u64 = barriers.iter().map(|b| b.0).sum();
    assert_eq!(outcome.seats[0].damage_absorbed, total);
    let broken: Vec<u64> = barriers
        .iter()
        .filter(|b| b.1.is_some_and(|d| d < SimDuration(15_000)))
        .map(|b| b.0)
        .collect();
    assert!(!broken.is_empty(), "{barriers:?}");
    assert!(broken.iter().all(|&s| s == broken[0]), "{broken:?}");
    // 5% of six million, give or take the run's 20% health jitter.
    assert!((240_000..=360_000).contains(&broken[0]), "{broken:?}");
}

#[test]
fn seats_see_their_channels_progress() {
    let mut b = bundle();
    let bolt = Arc::make_mut(&mut b.data)
        .spells
        .get_mut(&LIGHTNING_BOLT)
        .unwrap();
    bolt.cast = CastKind::Channel {
        duration: SimDuration(3000),
        ticks: 3,
        hasted: false,
        swings: false,
        tick_zero: false,
        on_end: Vec::new(),
    };
    bolt.speed = None;
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut seen = Vec::new();
    while let Turn::Decide(d) = turn {
        let choice = if let Some(c) = d.obs.channel {
            if d.reason == WakeReason::ChannelTick {
                seen.push(c);
            }
            Choice::Wait(Wait::NextEvent)
        } else if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::Cast {
                ability: LIGHTNING_BOLT,
                target: TargetSel::Primary,
                opts: CastOpts {
                    empower: None,
                    tick_wakes: true,
                },
            }
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        if seen.len() == 2 {
            break;
        }
        turn = env.step(choice).unwrap().next;
    }
    let tick = |ticks_done| ChannelObs {
        ticks_done,
        ticks_total: 3,
        next_tick: SimDuration(1000),
    };
    assert_eq!(seen, vec![tick(1), tick(2)]);
}

#[test]
fn seats_see_their_empowers_progress() {
    let mut b = bundle();
    let bolt = Arc::make_mut(&mut b.data)
        .spells
        .get_mut(&LIGHTNING_BOLT)
        .unwrap();
    bolt.cast = CastKind::Empower {
        stages: vec![SimDuration(1000), SimDuration(1750)],
        hasted: false,
        hold: SimDuration(2000),
        stage_effects: vec![Vec::new(), Vec::new()],
    };
    bolt.speed = None;
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut seen = Vec::new();
    while let Turn::Decide(d) = turn {
        let choice = if let Some(e) = d.obs.empower {
            if matches!(d.reason, WakeReason::EmpowerStage(_)) {
                seen.push(e);
            }
            Choice::Wait(Wait::NextEvent)
        } else if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::Cast {
                ability: LIGHTNING_BOLT,
                target: TargetSel::Primary,
                opts: CastOpts {
                    empower: None,
                    tick_wakes: true,
                },
            }
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        if seen.len() == 2 {
            break;
        }
        turn = env.step(choice).unwrap().next;
    }
    let stage = |stage, next_stage| EmpowerObs { stage, next_stage };
    assert_eq!(seen, vec![stage(1, Some(SimDuration(750))), stage(2, None)]);
}

/// A dummy with a magic ward and a 5 s vanishing act 10 s into its clock,
/// and Lightning Bolt stunning what it hits.
#[test]
fn seats_see_their_targets_stuns_and_wards_and_lose_it_while_it_hides() {
    const WARD: AuraId = AuraId(900_931);
    const VANISH: AuraId = AuraId(900_932);
    let mut b = bundle();
    let data = Arc::make_mut(&mut b.data);
    let mut ward = data.auras[&AuraId(910_001)].clone();
    ward.id = WARD;
    ward.duration = None;
    ward.modifiers.clear();
    ward.dispel = Some(DispelType::Magic);
    let mut vanish = ward.clone();
    vanish.id = VANISH;
    vanish.duration = Some(SimDuration(5000));
    vanish.dispel = None;
    vanish.untargetable = true;
    data.auras.insert(WARD, ward);
    data.auras.insert(VANISH, vanish);
    // The seat's own Flame Shock is magic too, but not the dummy's to lose.
    data.auras.get_mut(&AuraId(188389)).unwrap().dispel = Some(DispelType::Magic);
    data.spells
        .get_mut(&LIGHTNING_BOLT)
        .unwrap()
        .effects
        .push(Effect::Stun {
            duration: SimDuration(2000),
            target: EffectTarget::Target,
        });
    let enemies = Arc::make_mut(&mut b.enemies);
    let rule = |name: &str, when, aura| EnemyRule {
        name: EventName(name.into()),
        phase: None,
        when,
        repeat: None,
        action: EnemyAction::SelfAura(aura),
    };
    enemies
        .enemies
        .get_mut(&EnemyKey("target_dummy".into()))
        .unwrap()
        .rules = vec![
        rule("ward", Trigger::Now, WARD),
        rule("vanish", Trigger::Elapsed(SimDuration(10_000)), VANISH),
    ];
    let policy = Arc::clone(&b.arm(false).policies[0]);
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut stunned = 0;
    // Whether the seat had the dummy as its target, decision by decision
    // from when it first did, with repeats dropped.
    let mut seen: Vec<bool> = Vec::new();
    while let Turn::Decide(d) = turn {
        if let Some(t) = &d.obs.target {
            assert_eq!(t.dispellable, BTreeMap::from([(WARD, DispelType::Magic)]));
            if let Some(left) = t.stunned {
                assert!(left > SimDuration::ZERO && left <= SimDuration(2000));
                stunned += 1;
            }
        }
        let has = d.obs.target.is_some();
        if (has || !seen.is_empty()) && seen.last() != Some(&has) {
            seen.push(has);
        }
        turn = env.step(policy.act(&d)).unwrap().next;
    }
    assert!(stunned > 0);
    // Stuns hold the dummy's clock back, so it hides later than 10 s in,
    // but it hides once and comes back.
    assert_eq!(seen, vec![true, false, true]);
}

#[test]
fn seats_see_shields_as_a_share_of_health() {
    let mut b = bundle();
    let data = Arc::make_mut(&mut b.data);
    let shield = |id: u32, pct: f64| AuraDef {
        id: AuraId(id),
        name: format!("shield {id}"),
        duration: None,
        max_stacks: 1,
        refresh: RefreshRule::Replace,
        periodic: None,
        value: Some(AuraValue {
            kind: AuraValueKind::Absorb {
                school: SchoolMask(u8::MAX),
            },
            initial: Some(Coefficient::PctMaxHealth(pct)),
            cap: None,
            threshold: None,
            on_threshold: Vec::new(),
        }),
        modifiers: Vec::new(),
        listeners: Vec::new(),
        overrides: Vec::new(),
        on_expire: Vec::new(),
        cancelable: false,
        blocked_by: None,
        form: None,
        stealth: None,
        ends_with: None,
        persists_through_death: false,
        unique_per_source: false,
        prevents_death: None,
        ground: None,
        untargetable: false,
        dispel: None,
        self_resurrect: None,
    };
    let apply = |id: u32, target| Effect::ApplyAura {
        aura: AuraId(id),
        target,
        stacks: 1,
        duration: None,
        per_unit_spent: None,
    };
    for s in [shield(900_701, 5.0), shield(900_702, 10.0)] {
        data.auras.insert(s.id, s);
    }
    let bolt = data.spells.get_mut(&LIGHTNING_BOLT).unwrap();
    bolt.speed = None;
    bolt.effects.push(apply(900_701, EffectTarget::Target));
    bolt.effects.push(apply(900_702, EffectTarget::Caster));
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    while let Turn::Decide(d) = turn {
        if d.obs.absorb_pct > 0.0 {
            let target = d.obs.target.expect("a target");
            assert!(
                (d.obs.absorb_pct - 10.0).abs() < 1e-9,
                "{}",
                d.obs.absorb_pct
            );
            assert!(
                (target.absorb_pct - 5.0).abs() < 1e-9,
                "{}",
                target.absorb_pct
            );
            return;
        }
        let choice = if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::cast(LIGHTNING_BOLT)
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        turn = env.step(choice).unwrap().next;
    }
    panic!("no shield seen");
}

#[test]
fn seats_see_their_last_few_casts() {
    let b = bundle();
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut seen = 0;
    while let Turn::Decide(d) = turn {
        let recent = &d.obs.recent_casts;
        assert!(recent.len() >= seen && recent.len() <= 4, "{recent:?}");
        assert!(recent.iter().all(|&s| s == LIGHTNING_BOLT), "{recent:?}");
        seen = recent.len();
        if seen == 4 {
            return;
        }
        let choice = if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::cast(LIGHTNING_BOLT)
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        turn = env.step(choice).unwrap().next;
    }
    panic!("never saw four casts");
}

#[test]
fn seats_see_when_their_runes_are_back() {
    let mut b = bundle();
    b.templates[0].resources.push(ResourceDef {
        kind: ResourceKind::Runes,
        max: 6.0,
        initial: 6.0,
        regen_per_sec: 0.0,
        regen_hasted: false,
        recharge: Some(RechargeDef {
            concurrent: 3,
            period: SimDuration(10_000),
            hasted: true,
        }),
        out_of_combat: None,
    });
    Arc::make_mut(&mut b.data)
        .spells
        .get_mut(&LIGHTNING_BOLT)
        .unwrap()
        .costs
        .push(Cost {
            kind: ResourceKind::Runes,
            amount: 2.0,
            extra: 0.0,
            scaling: SpendScaling::None,
        });
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    while let Turn::Decide(d) = turn {
        let obs = &d.obs;
        if obs.resource(ResourceKind::Runes) == 4.0 {
            let refills = &obs.refills[&ResourceKind::Runes];
            assert_eq!(refills.len(), 2, "{refills:?}");
            assert_eq!(refills[0], refills[1]);
            assert!(refills[0] > SimDuration::ZERO && refills[0] <= SimDuration(10_000));
            assert_eq!(obs.time_to(ResourceKind::Runes, 4), SimDuration::ZERO);
            assert_eq!(obs.time_to(ResourceKind::Runes, 6), refills[1]);
            assert_eq!(obs.time_to(ResourceKind::Runes, 7), FOREVER);
            assert!(!obs.refills.contains_key(&ResourceKind::Mana));
            return;
        }
        let choice = if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::cast(LIGHTNING_BOLT)
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        turn = env.step(choice).unwrap().next;
    }
    panic!("never spent runes");
}

#[test]
fn closures_are_policies() {
    let b = bundle();
    let bolt_only = |d: &Decision<SeatObs>| {
        if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::cast(LIGHTNING_BOLT)
        } else {
            Choice::Wait(Wait::NextEvent)
        }
    };
    let mut arm = b.arm(false);
    let full = arm.rollout(Seed(1)).unwrap();
    arm.policies = vec![Arc::new(bolt_only)];
    let bolts = arm.rollout(Seed(1)).unwrap();
    assert!(bolts.completed);
    assert!(bolts.end_time > full.end_time);
}

const BLOODLUST: SpellId = SpellId(2825);
const SATED: AuraId = AuraId(57724);

/// Two Shamans with Bloodlust left to their policies.
fn lust_pair() -> Bundle {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs");
    let mut config = bundle().config;
    config.seats.push(config.seats[0].clone());
    config.externals.bloodlust = BloodlustMode::Policy;
    Bundle::from_config(config, &path).unwrap()
}

/// When each seat (by actor) gained `aura`.
fn gained(trace: &[TraceRecord], aura: AuraId) -> Vec<(SimTime, ActorId)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::AuraApplied {
                holder, aura: a, ..
            } if a == aura => Some((r.time, holder)),
            _ => None,
        })
        .collect()
}

#[test]
fn bloodlust_left_to_policies_waits_for_a_cast_and_sated_stops_the_second() {
    let b = lust_pair();
    let elemental = Arc::clone(&b.arm(false).policies[0]);
    // Seat 0 lusts 15 s in, seat 1 tries 25 s in; until then neither may.
    let lust_at = |seat: Seat| SimDuration(if seat == Seat(0) { 15_000 } else { 25_000 });
    let mut env = b.env(true);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut casts = Vec::new();
    while let Turn::Decide(mut d) = turn {
        let due = d.obs.combat_time.is_some_and(|t| t >= lust_at(d.seat));
        let choice = if due && d.legal.is_ready(BLOODLUST) {
            casts.push((d.obs.now, d.seat));
            Choice::cast(BLOODLUST)
        } else {
            d.legal.abilities.remove(&BLOODLUST);
            elemental.act(&d)
        };
        turn = env.step(choice).unwrap().next;
    }
    let trace = env.drain_trace();
    assert_eq!(casts.len(), 2, "{casts:?}");
    let first = casts[0].0;
    let both = |at| vec![(at, ActorId(0)), (at, ActorId(1))];
    // The first cast lusts and sates both seats at once; the second, with
    // both sated, does nothing.
    assert_eq!(gained(&trace, AuraId(2825)), both(first));
    assert_eq!(gained(&trace, SATED), both(first));
}

#[test]
fn the_elemental_script_lusts_at_the_pull_when_it_has_the_spell() {
    let b = lust_pair();
    let policies = b.arm(false).policies;
    let mut env = b.env(true);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut started = None;
    while let Turn::Decide(d) = turn {
        if d.obs.combat_time == Some(SimDuration::ZERO) {
            started.get_or_insert(d.obs.now);
        }
        turn = env
            .step(policies[usize::from(d.seat.0)].act(&d))
            .unwrap()
            .next;
    }
    let trace = env.drain_trace();
    let started = started.expect("a decision as combat starts");
    assert_eq!(
        gained(&trace, AuraId(2825)),
        vec![(started, ActorId(0)), (started, ActorId(1))]
    );
    // Which is what the automatic Bloodlust does.
    let mut auto = b.config.clone();
    auto.externals.bloodlust = BloodlustMode::Auto;
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs");
    let auto = Bundle::from_config(auto, &path).unwrap();
    let damage = |b: &Bundle| {
        let o = b.arm(false).rollout(Seed(1)).unwrap();
        o.seats.iter().map(|s| s.damage_done).collect::<Vec<_>>()
    };
    assert_eq!(damage(&b), damage(&auto));
}

const POWER_INFUSION: SpellId = SpellId(10060);
const INFUSED: AuraId = AuraId(10060);
const HEAVY_HITTER: AuraId = AuraId(900_941);

/// Three Shamans: the first can Power Infuse an ally, the third hits half
/// again as hard as the second.
#[test]
fn power_infusion_goes_to_the_ally_with_the_most_damage_and_allies_see_it() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs");
    let mut config = bundle().config;
    config.seats.push(config.seats[0].clone());
    config.seats.push(config.seats[0].clone());
    let mut b = Bundle::from_config(config, &path).unwrap();
    let data = Arc::make_mut(&mut b.data);
    let mut infused = data.auras[&AuraId(2825)].clone();
    infused.id = INFUSED;
    infused.name = "Power Infusion".into();
    infused.duration = Some(SimDuration(15_000));
    infused.blocked_by = None;
    let mut heavy = data.auras[&AuraId(910_001)].clone();
    heavy.id = HEAVY_HITTER;
    heavy.duration = None;
    heavy.modifiers[0].value = 50.0;
    let mut pi = data.spells[&BLOODLUST].clone();
    pi.id = POWER_INFUSION;
    pi.targeting = Targeting::Ally;
    pi.effects = vec![Effect::ApplyAura {
        aura: INFUSED,
        target: EffectTarget::Target,
        stacks: 1,
        duration: None,
        per_unit_spent: None,
    }];
    data.auras.insert(INFUSED, infused);
    data.auras.insert(HEAVY_HITTER, heavy);
    data.spells.insert(POWER_INFUSION, pi);
    b.templates[0].abilities.insert(POWER_INFUSION);
    b.templates[2].passive_auras.push(HEAVY_HITTER);
    let elemental = Arc::clone(&b.arm(false).policies[0]);
    let mut env = b.env(true);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut meters = None;
    let mut seen_cooling = false;
    while let Turn::Decide(d) = turn {
        let late = d.obs.combat_time.is_some_and(|t| t >= SimDuration(10_000));
        // Nobody dies: charges come every 10 minutes from one at the start.
        let due = d.obs.now.0 / 600_000;
        assert_eq!(
            d.obs.battle_res,
            BattleResObs {
                charges: (1 + due as u8).min(5),
                next_in: Some(SimTime((due + 1) * 600_000).saturating_since(d.obs.now)),
            }
        );
        let infuse = (d.seat == Seat(0) && late)
            .then(|| buff_top_ally(&d, POWER_INFUSION, INFUSED))
            .flatten();
        if infuse.is_some() {
            let seats: Vec<Seat> = d.obs.allies.iter().map(|a| a.seat).collect();
            assert_eq!(seats, vec![Seat(1), Seat(2)]);
            meters = Some((d.obs.allies[0].damage_done, d.obs.allies[1].damage_done));
        }
        if d.seat == Seat(1) {
            seen_cooling |= d.obs.allies[0]
                .cooldowns
                .get(&POWER_INFUSION)
                .is_some_and(|c| c.remaining > SimDuration::ZERO);
        }
        let choice = infuse.unwrap_or_else(|| elemental.act(&d));
        turn = env.step(choice).unwrap().next;
    }
    let trace = env.drain_trace();
    let (second, third) = meters.expect("seat 0 infused someone");
    assert!(third > second, "{second} {third}");
    assert_eq!(gained(&trace, INFUSED).len(), 1);
    assert_eq!(gained(&trace, INFUSED)[0].1, ActorId(2));
    assert!(
        seen_cooling,
        "seat 1 sees seat 0's Power Infusion cooling down"
    );
}

#[test]
fn unknown_policies_are_rejected() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/runs");
    let mut config = bundle().config;
    config.seats[0].policy = "nope".into();
    assert!(matches!(
        Bundle::from_config(config, &path),
        Err(RunError::UnknownPolicy { seat: 0, .. })
    ));
}

#[test]
fn slain_adds_count_toward_forces_but_despawned_ones_do_not() {
    let mut b = bundle();
    let enemies = Arc::make_mut(&mut b.enemies);
    let dummy = EnemyKey("target_dummy".into());
    let add = |key: &str, forces: u32| {
        let mut def = enemies.enemies[&dummy].clone();
        def.key = EnemyKey(key.into());
        def.kind = EnemyKind::Add;
        def.health = 20_000;
        def.forces = forces;
        def
    };
    let (imp, wisp) = (add("imp", 3), add("wisp", 5));
    enemies.enemies.insert(imp.key.clone(), imp);
    enemies.enemies.insert(wisp.key.clone(), wisp);
    let summon = |key: &str, count: u32, despawn_with_spawner: bool| EnemyRule {
        name: EventName(format!("summon {key}")),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::SpawnAdds {
            adds: vec![(EnemyKey(key.into()), count)],
            distance: None,
            despawn_with_spawner,
        },
    };
    enemies.enemies.get_mut(&dummy).unwrap().rules =
        vec![summon("imp", 2, false), summon("wisp", 1, true)];
    let run = b.source(false).episode(Seed(1)).setup.run;
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut seen = Vec::new();
    while let Turn::Decide(d) = turn {
        let state = env.state().unwrap();
        seen.push(PartyMeter.measure(state, &run).forces);
        let choice = if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::cast(LIGHTNING_BOLT)
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        turn = env.step(choice).unwrap().next;
    }
    let Turn::Done(outcome) = turn else {
        unreachable!()
    };
    assert!(outcome.completed, "{outcome:?}");
    let state = env.state().unwrap();
    // The bolts follow the primary target: the dummy, then the imps; the
    // wisp leaves with the dummy.
    assert_eq!(seen.iter().max(), Some(&3));
    assert_eq!(PartyMeter.measure(state, &run).forces, 6);
}

#[test]
fn seats_see_how_far_the_targets_packmates_stand_from_it() {
    let mut b = bundle();
    let enemies = Arc::make_mut(&mut b.enemies);
    let dummy = EnemyKey("target_dummy".into());
    for key in ["imp", "wisp"] {
        let mut def = enemies.enemies[&dummy].clone();
        def.key = EnemyKey(key.into());
        def.kind = EnemyKind::Add;
        def.rules = Vec::new();
        enemies.enemies.insert(def.key.clone(), def);
    }
    // The dummy stands 20 yards out; the wisp 10 yards behind it.
    let summon = |key: &str, distance: Option<f64>| EnemyRule {
        name: EventName(format!("summon {key}")),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::SpawnAdds {
            adds: vec![(EnemyKey(key.into()), 1)],
            distance: distance.map(portunus_core::Dist::Fixed),
            despawn_with_spawner: false,
        },
    };
    enemies.enemies.get_mut(&dummy).unwrap().rules =
        vec![summon("imp", None), summon("wisp", Some(30.0))];
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    while let Turn::Decide(d) = turn {
        if let Some(target) = d.obs.target.as_ref().filter(|t| t.neighbours.len() == 2) {
            assert_eq!(target.neighbours, vec![0.0, 10.0]);
            assert_eq!(d.obs.enemies_near_target(8.0), 2);
            assert_eq!(d.obs.enemies_near_target(10.0), 3);
            return;
        }
        let choice = if d.legal.is_ready(LIGHTNING_BOLT) {
            Choice::cast(LIGHTNING_BOLT)
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        turn = env.step(choice).unwrap().next;
    }
    panic!("never saw both adds beside the target");
}

#[test]
fn seats_see_how_far_they_can_stray_from_their_area_and_who_is_in_it() {
    let mut b = bundle();
    let data = Arc::make_mut(&mut b.data);
    let area = AuraId(900_703);
    data.auras.insert(
        area,
        AuraDef {
            id: area,
            name: "area".into(),
            duration: Some(SimDuration(60_000)),
            max_stacks: 1,
            refresh: RefreshRule::Replace,
            periodic: None,
            value: None,
            modifiers: Vec::new(),
            listeners: Vec::new(),
            overrides: Vec::new(),
            on_expire: Vec::new(),
            cancelable: false,
            blocked_by: None,
            form: None,
            stealth: None,
            ends_with: None,
            persists_through_death: false,
            unique_per_source: false,
            prevents_death: None,
            ground: Some(GroundDef { radius: 8.0 }),
            untargetable: false,
            dispel: None,
            self_resurrect: None,
        },
    );
    data.spells
        .get_mut(&LIGHTNING_BOLT)
        .unwrap()
        .effects
        .push(Effect::ApplyAura {
            aura: area,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
            per_unit_spent: None,
        });
    let mut env = b.env(false);
    let mut turn = env.reset(Seed(1)).unwrap();
    let mut walked = false;
    while let Turn::Decide(d) = turn {
        let choice = match d.obs.buffs.get(&area).and_then(|a| a.ground) {
            Some(g) if !walked => {
                // Placed at the dummy, which stands in it.
                assert_eq!(
                    g,
                    GroundObs {
                        yards_left: 8.0,
                        enemies: 1
                    }
                );
                walked = true;
                Choice::Move(MoveGoal::Yards(3.0))
            }
            Some(g) if d.obs.movement.is_none() => {
                // Movement ends on a whole millisecond, a hair past 3 yards.
                assert!((g.yards_left - 5.0).abs() < 0.01, "{g:?}");
                return;
            }
            _ if walked => Choice::Wait(Wait::NextEvent),
            _ if d.legal.is_ready(LIGHTNING_BOLT) => Choice::cast(LIGHTNING_BOLT),
            _ => Choice::Wait(Wait::NextEvent),
        };
        turn = env.step(choice).unwrap().next;
    }
    panic!("never saw the area");
}

#[test]
fn retribution_runs_execute_their_hero_kits() {
    hero_forks_agree("retribution_templar_dummy.ron", HeroTreeId(48));
    hero_forks_agree("retribution_herald_dummy.ron", HeroTreeId(50));

    let runs: [(&str, &[&str]); 2] = [
        (
            "retribution_templar_dummy",
            &[
                "Wake of Ashes",
                "Hammer of Light",
                "Empyrean Hammer",
                "Execution Sentence",
                "Divine Toll",
                "Blade of Justice",
                "Final Verdict",
                "Judgment",
                "Hammer of Wrath",
                "Crusading Strikes",
            ],
        ),
        (
            "retribution_herald_dummy",
            &[
                "Wake of Ashes",
                "Dawnlight",
                "Sun's Avatar",
                "Execution Sentence",
                "Divine Toll",
                "Blade of Justice",
                "Final Verdict",
                "Judgment",
                "Hammer of Wrath",
                "Crusading Strikes",
            ],
        ),
    ];
    for (run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn death_knight_runs_execute_their_hero_kits() {
    hero_forks_agree("frost_deathbringer_dummy.ron", HeroTreeId(33));
    hero_forks_agree("frost_rider_dummy.ron", HeroTreeId(32));
    hero_forks_agree("unholy_rider_dummy.ron", HeroTreeId(32));
    hero_forks_agree("unholy_sanlayn_dummy.ron", HeroTreeId(31));

    let runs: [(&str, &[&str]); 4] = [
        (
            "frost_deathbringer_dummy",
            &[
                "Pillar of Frost",
                "Reaper's Mark",
                "Wave of Souls",
                "Reaper's Mark (Explosion)",
                "Exterminate (First Scythe)",
                "Obliterate",
                "Frost Strike",
                "Howling Blast",
                "Frost Fever",
                "Remorseless Winter (Damage)",
            ],
        ),
        (
            "frost_rider_dummy",
            &[
                "Pillar of Frost",
                "Obliterate",
                "Frost Strike",
                "Howling Blast",
                "Frost Fever",
                "Remorseless Winter (Damage)",
                "King Thoras Trollbane: Obliterate (Trollbane)",
            ],
        ),
        (
            "unholy_rider_dummy",
            &[
                "Raise Dead",
                "Army of the Dead",
                "Dark Transformation",
                "Putrefy",
                "Festering Strike",
                "Festering Scythe",
                "Scourge Strike",
                "Necrotic Coil",
                "Virulent Plague",
                "Dread Plague",
                "High Inquisitor Sally Whitemane: Death Coil (Whitemane)",
            ],
        ),
        (
            "unholy_sanlayn_dummy",
            &[
                "Raise Dead",
                "Army of the Dead",
                "Dark Transformation",
                "Vampiric Strike",
                "Infliction of Sorrow",
                "Blood Beast: Corrupted Blood",
                "Putrefy",
                "Festering Strike",
                "Festering Scythe",
                "Virulent Plague",
                "Dread Plague",
            ],
        ),
    ];
    for (run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn hunter_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "beast_mastery_pack_leader_dummy",
            &[
                "Bestial Wrath",
                "Barbed Shot",
                "Cobra Shot",
                "Stomp",
                "Bloodshed (Periodic)",
                "Stampede",
                "Hunter Pet: Kill Command (Pet)",
                "Animal Companion: Kill Command (Pet)",
            ],
        ),
        (
            "beast_mastery_dark_ranger_dummy",
            &[
                "Bestial Wrath",
                "Barbed Shot",
                "Cobra Shot",
                "Black Arrow",
                "Bleak Powder",
                "Black Arrow (Withering Fire)",
                "Wailing Arrow",
                "Shadow Thrash",
                "Hunter Pet: Kill Command (Pet)",
            ],
        ),
        (
            "marksmanship_sentinel_dummy",
            &[
                "Trueshot",
                "Aimed Shot",
                "Rapid Fire",
                "Rapid Fire (Shot)",
                "Explosive Shot",
                "Volley",
                "Moonlight Chakram",
                "Lunar Storm",
            ],
        ),
        (
            "marksmanship_dark_ranger_dummy",
            &[
                "Trueshot",
                "Aimed Shot",
                "Rapid Fire (Shot)",
                "Explosive Shot",
                "Volley",
                "Black Arrow",
                "Bleak Powder",
                "Wailing Arrow",
                "Blighted Arrow",
            ],
        ),
        (
            "survival_pack_leader_dummy",
            &[
                "Takedown",
                "Wildfire Bomb",
                "Wildfire Bomb (Explosion)",
                "Boomstick",
                "Boomstick (Blast)",
                "Raptor Swipe",
                "Strike as One",
                "Stampede",
                "Hunter Pet: Kill Command (Survival Pet)",
            ],
        ),
        (
            "survival_sentinel_dummy",
            &[
                "Takedown",
                "Wildfire Bomb",
                "Boomstick",
                "Raptor Swipe",
                "Strike as One",
                "Moonlight Chakram",
                "Lunar Storm",
                "Hunter Pet: Kill Command (Survival Pet)",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn rogue_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "assassination_deathstalker_dummy",
            &[
                "Mutilate",
                "Garrote",
                "Rupture",
                "Envenom",
                "Deathmark",
                "Kingsbane",
                "Implacable Strikes",
                "Deadly Poison (Periodic)",
                "Deathstalker's Mark",
            ],
        ),
        (
            "assassination_fatebound_dummy",
            &[
                "Mutilate",
                "Garrote",
                "Rupture",
                "Envenom",
                "Deathmark",
                "Kingsbane",
                "Implacable Strikes",
                "Fatebound Coin (Tails)",
            ],
        ),
        (
            "outlaw_trickster_dummy",
            &[
                "Adrenaline Rush",
                "Roll the Bones",
                "Sinister Strike",
                "Pistol Shot",
                "Between the Eyes",
                "Dispatch",
                "Scoundrel Strike",
                "Main Gauche",
                "Unseen Blade",
                "Coup de Grace",
            ],
        ),
        (
            "outlaw_fatebound_dummy",
            &[
                "Adrenaline Rush",
                "Roll the Bones",
                "Sinister Strike",
                "Pistol Shot",
                "Between the Eyes",
                "Dispatch",
                "Scoundrel Strike",
                "Main Gauche",
                "Fatebound Coin (Tails)",
            ],
        ),
        (
            "subtlety_deathstalker_dummy",
            &[
                "Shadow Blades",
                "Shadow Dance",
                "Shadowstrike",
                "Goremaw's Bite",
                "Secret Technique",
                "Eviscerate",
                "Eviscerate (Shadow)",
                "Deathstalker's Mark",
                "Corrupt the Blood (Periodic)",
            ],
        ),
        (
            "subtlety_trickster_dummy",
            &[
                "Shadow Blades",
                "Shadow Dance",
                "Shadowstrike",
                "Goremaw's Bite",
                "Secret Technique",
                "Eviscerate",
                "Unseen Blade",
                "Coup de Grace",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn mage_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "arcane_spellslinger_dummy",
            &[
                "Arcane Surge",
                "Touch of the Magi",
                "Prismatic Bolt",
                "Arcane Orb",
                "Arcane Blast",
                "Arcane Missiles",
                "Arcane Barrage",
                "Arcane Splinter",
                "Splinterstorm",
            ],
        ),
        (
            "arcane_sunfury_dummy",
            &[
                "Arcane Surge",
                "Touch of the Magi",
                "Prismatic Bolt",
                "Arcane Orb",
                "Arcane Blast",
                "Arcane Missiles",
                "Arcane Barrage",
                "Meteorite",
                "Arcane Phoenix: Spellfire Bolt",
            ],
        ),
        (
            "fire_sunfury_dummy",
            &[
                "Combustion",
                "Meteor",
                "Fire Blast",
                "Pyroblast",
                "Ignite (Periodic)",
                "Meteorite",
                "Arcane Phoenix: Spellfire Bolt",
            ],
        ),
        (
            "fire_frostfire_dummy",
            &[
                "Combustion",
                "Meteor",
                "Frostfire Bolt",
                "Fire Blast",
                "Pyroblast",
                "Ignite (Periodic)",
                "Frostfire Infusion",
                "Comet Storm (Isothermic Core)",
            ],
        ),
        (
            "frost_spellslinger_dummy",
            &[
                "Frozen Orb",
                "Comet Storm",
                "Ray of Frost",
                "Flurry",
                "Ice Lance",
                "Glacial Spike",
                "Frost Splinter",
                "Splinterstorm",
                "Shatter",
                "Hand of Frost",
            ],
        ),
        (
            "frost_frostfire_dummy",
            &[
                "Frozen Orb",
                "Comet Storm",
                "Meteor (Impact)",
                "Ray of Frost",
                "Ice Lance",
                "Glacial Spike",
                "Shatter",
                "Hand of Frost",
                "Frostfire Infusion",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn warlock_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "affliction_hellcaller_dummy",
            &[
                "Haunt",
                "Agony",
                "Wither",
                "Wither (Periodic)",
                "Wither (Burst)",
                "Malevolence",
                "Dark Harvest",
                "Summon Darkglare",
                "Unstable Affliction",
                "Shadow of Nathreza (Bolt)",
                "Sayaad: Lash of Pain",
            ],
        ),
        (
            "affliction_soul_harvester_dummy",
            &[
                "Haunt",
                "Agony",
                "Corruption",
                "Corruption (Periodic)",
                "Dark Harvest",
                "Summon Darkglare",
                "Unstable Affliction",
                "Kouros's Cleave",
                "Soul Anathema (Periodic)",
                "Sayaad: Lash of Pain",
            ],
        ),
        (
            "demonology_diabolist_dummy",
            &[
                "Power Siphon",
                "Grimoire: Imp Lord",
                "Summon Doomguard",
                "Call Dreadstalkers",
                "Summon Demonic Tyrant",
                "Hand of Gul'dan",
                "Demonbolt",
                "Felguard: Legion Strike",
                "Wild Imp: Fel Firebolt",
            ],
        ),
        (
            "demonology_soul_harvester_dummy",
            &[
                "Power Siphon",
                "Grimoire: Imp Lord",
                "Summon Doomguard",
                "Call Dreadstalkers",
                "Summon Demonic Tyrant",
                "Hand of Gul'dan",
                "Demonbolt",
                "Kouros's Cleave",
                "Soul Anathema (Periodic)",
                "Felguard: Legion Strike",
                "Wild Imp: Fel Firebolt",
            ],
        ),
        (
            "destruction_hellcaller_dummy",
            &[
                "Summon Infernal",
                "Malevolence",
                "Cataclysm",
                "Wither (Periodic)",
                "Wither (Burst)",
                "Soul Fire",
                "Conflagrate",
                "Chaos Bolt",
                "Embers of Nihilam (Volley)",
                "Imp: Firebolt",
            ],
        ),
        (
            "destruction_diabolist_dummy",
            &[
                "Summon Infernal",
                "Cataclysm",
                "Immolate (Periodic)",
                "Soul Fire",
                "Conflagrate",
                "Chaos Bolt",
                "Embers of Nihilam (Volley)",
                "Imp: Firebolt",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn priest_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "shadow_voidweaver_dummy",
            &[
                "Tentacle Slam",
                "Voidwraith",
                "Voidform",
                "Void Torrent",
                "Void Blast",
                "Void Volley",
                "Shadow Word: Madness",
                "Shadowy Apparition",
                "Void Apparition",
                "Entropic Rift: Entropic Rift (Pulse)",
                "Voidwraith: Void Flay (Voidwraith)",
            ],
        ),
        (
            "shadow_archon_dummy",
            &[
                "Tentacle Slam",
                "Mindbender",
                "Halo",
                "Halo (Damage)",
                "Voidform",
                "Void Volley",
                "Mind Blast",
                "Mind Flay: Insanity",
                "Shadow Word: Madness",
                "Shadowy Apparition",
                "Void Apparition",
                "Mindbender: Shadowcrawl",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn monk_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "windwalker_celestial_dummy",
            &[
                "Invoke Xuen, the White Tiger",
                "Zenith",
                "Zenith Stomp",
                "Celestial Conduit",
                "Unity Within",
                "Whirling Dragon Punch",
                "Strike of the Windlord",
                "Fists of Fury (Hit)",
                "Rushing Wind Kick",
                "Rising Sun Kick",
                "Tigereye Strike",
                "Xuen, the White Tiger: Crackling Tiger Lightning",
            ],
        ),
        (
            "windwalker_shado_pan_dummy",
            &[
                "Zenith",
                "Zenith Stomp",
                "Whirling Dragon Punch",
                "Strike of the Windlord",
                "Fists of Fury (Hit)",
                "Rushing Wind Kick",
                "Rising Sun Kick",
                "Tigereye Strike",
                "Flurry Strikes",
                "Wisdom of the Wall (Proc)",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn druid_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "balance_keeper_dummy",
            &[
                "Moonfire",
                "Sunfire",
                "Force of Nature",
                "Fury of Elune",
                "Celestial Alignment",
                "Starsurge",
                "Ascendant Star",
                "Wrath",
                "Dream Burst",
                "Treant: Dream Burst",
            ],
        ),
        (
            "balance_elunes_chosen_dummy",
            &[
                "Moonfire",
                "Sunfire",
                "Force of Nature",
                "Fury of Elune",
                "Full Moon (Boundless Moonlight)",
                "Celestial Alignment",
                "Starsurge",
                "Ascendant Star",
                "Starfire",
            ],
        ),
        (
            "feral_druid_of_the_claw_dummy",
            &[
                "Tiger's Fury",
                "Chomp",
                "Berserk",
                "Ravage",
                "Dreadful Wound (Periodic)",
                "Frantic Frenzy",
                "Rip",
                "Rake",
                "Unseen Strike",
            ],
        ),
        (
            "feral_wildstalker_dummy",
            &[
                "Tiger's Fury",
                "Chomp",
                "Berserk",
                "Frantic Frenzy",
                "Rip",
                "Rake",
                "Bloodseeker Vines (Periodic)",
                "Unseen Strike",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn demon_hunter_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "havoc_fel_scarred_dummy",
            &[
                "The Hunt",
                "Eternal Hunt (Strike)",
                "Essence Break",
                "Eye Beam",
                "Metamorphosis",
                "Death Sweep",
                "Annihilation",
                "Demonsurge",
                "Immolation Aura (Tick)",
            ],
        ),
        (
            "havoc_aldrachi_reaver_dummy",
            &[
                "The Hunt",
                "Eternal Hunt (Strike)",
                "Reaver's Glaive",
                "Fury of the Aldrachi",
                "Essence Break",
                "Eye Beam",
                "Metamorphosis",
                "Death Sweep",
                "Annihilation",
            ],
        ),
        (
            "devourer_annihilator_dummy",
            &[
                "Metamorphosis",
                "The Hunt (Devourer)",
                "Collapsing Star",
                "Midnight (Detonation)",
                "Voidfall Meteor",
                "World Killer",
                "Void Ray",
                "Eradicate",
                "Cull",
                "Devour",
            ],
        ),
        (
            "devourer_void_scarred_dummy",
            &[
                "Metamorphosis",
                "The Hunt (Devourer)",
                "Collapsing Star",
                "Midnight (Detonation)",
                "Demonsurge",
                "Void Ray",
                "Eradicate",
                "Cull",
                "Devour",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}

#[test]
fn evoker_runs_execute_their_hero_kits() {
    let runs: &[(&str, &[&str])] = &[
        (
            "devastation_scalecommander_dummy",
            &[
                "Dragonrage",
                "Eternity Surge",
                "Fire Breath",
                "Deep Breath",
                "Disintegrate",
                "Rising Fury (Blast)",
                "Bombardments (Explosion)",
            ],
        ),
        (
            "devastation_flameshaper_dummy",
            &[
                "Dragonrage",
                "Eternity Surge",
                "Fire Breath",
                "Deep Breath",
                "Disintegrate",
                "Rising Fury (Blast)",
                "Consume Flame",
            ],
        ),
        (
            "augmentation_scalecommander_dummy",
            &[
                "Blistering Scales",
                "Prescience",
                "Ebon Might",
                "Breath of Eons",
                "Fire Breath",
                "Upheaval",
                "Deep Breath",
                "Eruption",
                "Duplicate (Echo)",
                "Bombardments (Explosion)",
            ],
        ),
        (
            "augmentation_chronowarden_dummy",
            &[
                "Blistering Scales",
                "Prescience",
                "Ebon Might",
                "Breath of Eons",
                "Fire Breath",
                "Upheaval",
                "Deep Breath",
                "Eruption",
                "Duplicate (Echo)",
                "Afterimage",
            ],
        ),
    ];
    for &(run, expected) in runs {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../data/runs/{run}.ron"));
        let bd = Bundle::load(&path)
            .unwrap()
            .breakdown(SeedSet { first: 0, count: 4 })
            .unwrap();
        for name in expected {
            assert!(
                bd.rows
                    .iter()
                    .any(|r| r.name == *name && (r.casts > 0.0 || r.hits > 0.0)),
                "{run} never used {name}: {:#?}",
                bd.rows
            );
        }
    }
}
