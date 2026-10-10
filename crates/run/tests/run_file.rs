//! The checked-in run files, end to end.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{
    AuraId, EnemyKey, EventName, HeroTreeId, PetId, Seed, SimDuration, SpellId, Trigger,
};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, MoveGoal, Readiness, StateView, TargetSel, Wait, WakeReason,
};
use portunus_env::{Decision, Env, EpisodeSource, ProgressMeter, Turn};
use portunus_eval::{Arm, LocalRunner, Metric, Runner, SeedSet};
use portunus_gamedata::aura::{AuraDef, AuraValue, AuraValueKind, RefreshRule};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget};
use portunus_gamedata::enemy::{EnemyAction, EnemyKind, EnemyRule};
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::stats::{
    Cost, RechargeDef, ResourceDef, ResourceKind, SchoolMask, SpendScaling,
};
use portunus_run::{Bundle, ChannelObs, EmpowerObs, PartyMeter, RunError, SeatObs, FOREVER};

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
