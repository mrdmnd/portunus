//! The checked-in data files load, cross-check, compile, and sample.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{
    AuraId, Dist, EnemyKey, EventName, HeroTreeId, Seed, SimDuration, SpecId, SpellId, TalentId,
    Trigger,
};
use portunus_gamedata::aura::{
    AuraValue, AuraValueKind, BankDraw, DispelType, GroundDef, PreventDeath,
};
use portunus_gamedata::effect::{
    Coefficient, CountForm, CountScale, Effect, EffectTarget, ListenFor, Listener, ModKind,
    ModScope, Modifier, Predicate, ProcChance, TargetCount,
};
use portunus_gamedata::enemy::{EnemyAction, EnemyKind, EnemyRule};
use portunus_gamedata::spell::{CastKind, CooldownDef, Requirement};
use portunus_gamedata::stats::{
    RatingFalloff, RechargeDef, ResourceDef, ResourceKind, SchoolMask, Stat,
};
use portunus_gamedata::talent::{TalentNode, TalentTree};
use portunus_gamedata::{EnemyData, GameData, HeroTreeDef};
use portunus_ingest::{
    check_enemy_data, check_game_data, read_ron, DataIssue, EnemyDataSource, GameDataSource, Owner,
    RonFile,
};
use portunus_loadout::{Compiler, Loadout, LoadoutCompiler};
use portunus_scenario::resolved::Segment;
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

fn game() -> GameData {
    GameDataSource::load(&RonFile::new(data_dir().join("game.ron"))).expect("game.ron")
}

fn enemies() -> EnemyData {
    EnemyDataSource::load(&RonFile::new(data_dir().join("enemies.ron"))).expect("enemies.ron")
}

#[test]
fn tables_have_no_dangling_references() {
    let game = game();
    let enemies = enemies();
    assert_eq!(check_game_data(&game), vec![]);
    assert_eq!(check_enemy_data(&enemies, &game), vec![]);
    assert_eq!(game.build, enemies.build);
}

#[test]
fn elemental_loadout_compiles() {
    let game = game();
    let loadout: Loadout = read_ron(&data_dir().join("loadouts/elemental.ron")).expect("loadout");
    let template = Compiler.compile(&game, &loadout).expect("compiles");

    let abilities: Vec<SpellId> = template.abilities.iter().copied().collect();
    assert_eq!(
        abilities,
        [8042, 51505, 79206, 188196, 188389, 188443, 192063, 192106, 196840]
            .map(SpellId)
            .to_vec()
    );
    assert_eq!(
        template.passive_auras,
        [168534, 77756, 51505, 60188].map(AuraId).to_vec()
    );
    // 250 crit rating from the robe at 700 rating per percent.
    let expected_crit = 5.0 + 250.0 / 700.0;
    assert!((template.derived.crit_pct - expected_crit).abs() < 1e-9);
    assert!(template.derived.spell_power > 2000.0);
}

#[test]
fn stormbringer_loadout_compiles() {
    let game = game();
    let loadout: Loadout =
        read_ron(&data_dir().join("loadouts/elemental_stormbringer.ron")).expect("loadout");
    assert_eq!(Compiler.validate(&game, &loadout), vec![]);
    let template = Compiler.compile(&game, &loadout).expect("compiles");
    assert_eq!(template.hero_tree, Some(HeroTreeId(54)));
    assert!(template.abilities.contains(&SpellId(191634)), "Stormkeeper");
    for aura in [
        454009, 454391, 454021, 1264762, 455110, 455096, 454026, 1264691, 455129,
    ] {
        assert!(template.passive_auras.contains(&AuraId(aura)), "{aura}");
    }
}

#[test]
fn farseer_loadout_compiles() {
    let game = game();
    let loadout: Loadout =
        read_ron(&data_dir().join("loadouts/elemental_farseer.ron")).expect("loadout");
    assert_eq!(Compiler.validate(&game, &loadout), vec![]);
    let template = Compiler.compile(&game, &loadout).expect("compiles");
    assert_eq!(template.hero_tree, Some(HeroTreeId(55)));
    assert!(
        template.abilities.contains(&SpellId(443454)),
        "Ancestral Swiftness"
    );
    for aura in [
        443450, 443423, 443445, 443418, 1270446, 443451, 443448, 1270447, 443447, 443446, 448861,
    ] {
        assert!(template.passive_auras.contains(&AuraId(aura)), "{aura}");
    }
}

#[test]
fn arms_loadout_compiles() {
    let game = game();
    let loadout: Loadout = read_ron(&data_dir().join("loadouts/arms.ron")).expect("loadout");
    assert_eq!(Compiler.validate(&game, &loadout), vec![]);
    let template = Compiler.compile(&game, &loadout).expect("compiles");
    assert_eq!(template.hero_tree, None);
    // Massacre's Execute in place of the plain one.
    for spell in [12294, 7384, 167105, 845, 227847, 107574, 281000, 1464, 100] {
        assert!(template.abilities.contains(&SpellId(spell)), "{spell}");
    }
    assert!(!template.abilities.contains(&SpellId(163201)));
    let held = |aura: u32| {
        template
            .passive_auras
            .iter()
            .filter(|&&a| a == AuraId(aura))
            .count()
    };
    // A copy per rank: Strength of Arms, Sharpened Blades, Bloodborne,
    // Critical Thinking, and Master of Warfare's middle two ranks.
    for aura in [400803, 383341, 383287, 389306, 1269306] {
        assert_eq!(held(aura), 2, "{aura}");
    }
    for aura in [137049, 1258398, 386164, 29725, 1269314, 1269307] {
        assert_eq!(held(aura), 1, "{aura}");
    }
    let weapon = template.main_hand.expect("a two-hander");
    assert_eq!(weapon.speed.millis(), 3600);
}

#[test]
fn dummy_scenario_samples_deterministically() {
    let spec: ScenarioSpec =
        read_ron(&data_dir().join("scenarios/target_dummy.ron")).expect("scenario");
    let sampler = Sampler::new(spec, Arc::new(enemies())).expect("valid scenario");
    let a = sampler.sample(Seed(7));
    assert_eq!(a, sampler.sample(Seed(7)));
    assert_ne!(a, sampler.sample(Seed(8)));

    let [Segment::Travel { .. }, Segment::Combat(combat)] = a.segments.as_slice() else {
        panic!("expected travel then combat, got {:?}", a.segments);
    };
    let [dummy] = combat.spawns.as_slice() else {
        panic!("expected one spawn");
    };
    assert_eq!(dummy.label.0, "target_dummy#1");
    // A million, give or take a fifth from run to run.
    assert!((800_000..=1_200_000).contains(&dummy.max_health));
    assert_ne!(dummy.max_health, 1_000_000);
}

#[test]
fn broken_references_are_reported() {
    let mut game = game();
    game.auras.remove(&AuraId(77762));
    if let Some(spell) = game.spells.remove(&SpellId(8042)) {
        game.spells.insert(SpellId(9999), spell);
    }
    let issues = check_game_data(&game);
    assert!(issues.contains(&DataIssue::KeyMismatch(Owner::Spell(SpellId(9999)))));
    assert!(issues.contains(&DataIssue::UnknownAura {
        owner: Owner::Aura(AuraId(77756)),
        aura: AuraId(77762),
    }));
    assert!(issues.contains(&DataIssue::UnknownSpell {
        owner: Owner::Spec(SpecId(262)),
        spell: SpellId(8042),
    }));
}

#[test]
fn channels_without_ticks_or_duration_are_reported() {
    let mut game = game();
    let channel = |duration: u32, ticks: u8| CastKind::Channel {
        duration: SimDuration(duration),
        ticks,
        hasted: true,
        swings: false,
        tick_zero: false,
        on_end: Vec::new(),
    };
    let bolt = SpellId(188196);
    game.spells.get_mut(&bolt).unwrap().cast = channel(3000, 3);
    assert!(!check_game_data(&game).contains(&DataIssue::InvalidChannel(bolt)));
    for broken in [channel(3000, 0), channel(0, 3)] {
        game.spells.get_mut(&bolt).unwrap().cast = broken;
        assert!(check_game_data(&game).contains(&DataIssue::InvalidChannel(bolt)));
    }
}

#[test]
fn what_a_channel_does_as_it_ends_is_checked() {
    let mut game = game();
    let ghost = AuraId(999_996);
    let bolt = SpellId(188196);
    game.spells.get_mut(&bolt).unwrap().cast = CastKind::Channel {
        duration: SimDuration(3000),
        ticks: 3,
        hasted: true,
        swings: false,
        tick_zero: true,
        on_end: vec![Effect::ApplyAura {
            aura: ghost,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
            per_unit_spent: None,
        }],
    };
    assert!(check_game_data(&game).contains(&DataIssue::UnknownAura {
        owner: Owner::Spell(bolt),
        aura: ghost,
    }));
}

#[test]
fn pseudo_random_chances_must_be_a_real_chance() {
    let mut game = game();
    let holder = AuraId(77756);
    let owner = Owner::Aura(holder);
    let mut reported = |chance: f64| {
        let def = game.auras.get_mut(&holder).unwrap();
        def.listeners = vec![Listener {
            on: ListenFor::Swing {
                hand: None,
                crit_only: true,
            },
            chance: ProcChance::Prd { chance },
            internal_cooldown: None,
            per_unit: false,
            shared_with: None,
            condition: None,
            effects: Vec::new(),
        }];
        check_game_data(&game).contains(&DataIssue::InvalidChance(owner.clone()))
    };
    for (chance, bad) in [
        (0.25, false),
        (1.0, false),
        (0.0, true),
        (1.5, true),
        (f64::NAN, true),
    ] {
        assert_eq!(reported(chance), bad, "{chance}");
    }
}

#[test]
fn malformed_empowers_are_reported() {
    let mut game = game();
    let ms = |v: &[u32]| v.iter().map(|&m| SimDuration(m)).collect::<Vec<_>>();
    let empower = |stages: &[u32], effects: usize| CastKind::Empower {
        stages: ms(stages),
        hasted: true,
        hold: SimDuration(2000),
        stage_effects: vec![Vec::new(); effects],
    };
    let bolt = SpellId(188196);
    game.spells.get_mut(&bolt).unwrap().cast = empower(&[1000, 1750, 2500], 3);
    assert!(!check_game_data(&game).contains(&DataIssue::InvalidEmpower(bolt)));
    for broken in [
        empower(&[], 0),
        empower(&[0, 1000], 2),
        empower(&[1000, 1000], 2),
        empower(&[1000, 1750], 1),
    ] {
        game.spells.get_mut(&bolt).unwrap().cast = broken;
        assert!(check_game_data(&game).contains(&DataIssue::InvalidEmpower(bolt)));
    }
}

#[test]
fn categories_whose_spells_disagree_are_reported() {
    let mut game = game();
    let cd = |duration: u32, charges: u8, hasted: bool| CooldownDef {
        duration: SimDuration(duration),
        charges,
        hasted,
        category: Some(7),
    };
    let (a, b) = (SpellId(188196), SpellId(8042));
    game.spells.get_mut(&a).unwrap().cooldown = Some(cd(30_000, 1, false));
    game.spells.get_mut(&b).unwrap().cooldown = Some(cd(30_000, 1, false));
    assert!(!check_game_data(&game).contains(&DataIssue::CategoryMismatch(7)));
    for broken in [
        cd(20_000, 1, false),
        cd(30_000, 2, false),
        cd(30_000, 1, true),
    ] {
        game.spells.get_mut(&b).unwrap().cooldown = Some(broken);
        assert!(check_game_data(&game).contains(&DataIssue::CategoryMismatch(7)));
    }
}

#[test]
fn aura_values_with_non_positive_limits_are_reported() {
    let mut game = game();
    let aura = AuraId(77762);
    let set = |game: &mut GameData, initial, cap, threshold| {
        game.auras.get_mut(&aura).unwrap().value = Some(AuraValue {
            kind: AuraValueKind::Counter,
            initial,
            cap,
            threshold,
            on_threshold: Vec::new(),
        });
    };
    let good = Some(Coefficient::SpellPower(1.0));
    set(&mut game, good, good, good);
    assert!(!check_game_data(&game).contains(&DataIssue::InvalidAuraValue(aura)));
    let bad = Some(Coefficient::Flat(0.0));
    for (initial, cap, threshold) in [(bad, None, None), (None, bad, None), (None, None, bad)] {
        set(&mut game, initial, cap, threshold);
        assert!(check_game_data(&game).contains(&DataIssue::InvalidAuraValue(aura)));
    }
}

#[test]
fn predicates_that_cannot_hold_meaningfully_are_reported() {
    let mut game = game();
    let flame_shock = SpellId(188389);
    let owner = Owner::Spell(flame_shock);
    let aura = AuraId(77762);
    let mut reported = |when: Predicate| {
        let effects = &mut game.spells.get_mut(&flame_shock).unwrap().effects;
        effects.retain(|e| !matches!(e, Effect::If { .. }));
        effects.push(Effect::If {
            when,
            then: Vec::new(),
            otherwise: Vec::new(),
        });
        check_game_data(&game).contains(&DataIssue::InvalidPredicate(owner.clone()))
    };
    let stacks = |stacks| Predicate::CasterStacksAtLeast { aura, stacks };
    let recent = |count| Predicate::RecentCasts {
        spell: flame_shock,
        count,
    };
    for (when, bad) in [
        (Predicate::TargetHpAbove(0.7), false),
        (Predicate::TargetHpAbove(70.0), true),
        (Predicate::CasterHpBelow(-0.1), true),
        (Predicate::TargetHpBelow(f64::NAN), true),
        (stacks(1), false),
        (stacks(0), true),
        (recent(1), false),
        (recent(4), false),
        (recent(0), true),
        (recent(5), true),
        (
            Predicate::AuraValueAtLeast {
                aura,
                value: f64::INFINITY,
            },
            true,
        ),
        (Predicate::TargetHpBelowCasterMaxHp, false),
        (Predicate::EnemiesAtLeast(2), false),
        (Predicate::EnemiesAtLeast(0), true),
        (Predicate::TargetIsCurrent, false),
    ] {
        assert_eq!(reported(when), bad, "{when:?}");
    }
}

#[test]
fn requirements_that_cannot_be_met_meaningfully_are_reported() {
    let mut game = game();
    let flame_shock = SpellId(188389);
    let mut reported = |r: Requirement| {
        game.spells.get_mut(&flame_shock).unwrap().requires = vec![r];
        check_game_data(&game).contains(&DataIssue::InvalidRequirement(flame_shock))
    };
    let stacks = |stacks| Requirement::CasterStacksAtLeast {
        aura: AuraId(77762),
        stacks,
    };
    for (r, bad) in [
        (Requirement::TargetHpAtMost(0.2), false),
        (Requirement::TargetHpAtMost(20.0), true),
        (Requirement::TargetHpAtLeast(f64::NAN), true),
        (stacks(1), false),
        (stacks(0), true),
    ] {
        assert_eq!(reported(r.clone()), bad, "{r:?}");
    }
}

#[test]
fn recharging_resources_must_be_whole_units_without_linear_regen() {
    let mut game = game();
    let elemental = SpecId(262);
    let owner = Owner::Spec(elemental);
    let runes = ResourceDef {
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
    };
    let mut reported = |r: ResourceDef| {
        game.specs.get_mut(&elemental).unwrap().resources = vec![r];
        check_game_data(&game).contains(&DataIssue::InvalidRecharge(owner.clone()))
    };
    let with = |edit: fn(&mut ResourceDef)| {
        let mut r = runes;
        edit(&mut r);
        r
    };
    assert!(!reported(runes));
    assert!(reported(with(|r| r.regen_per_sec = 1.0)));
    assert!(reported(with(|r| r.max = 5.5)));
    assert!(reported(with(|r| r.initial = 7.0)));
    assert!(reported(with(|r| {
        if let Some(rc) = r.recharge.as_mut() {
            rc.concurrent = 0;
        }
    })));
    assert!(reported(with(|r| {
        if let Some(rc) = r.recharge.as_mut() {
            rc.period = SimDuration::ZERO;
        }
    })));
}

#[test]
fn out_of_combat_rates_must_be_finite_and_linear() {
    let mut game = game();
    let elemental = SpecId(262);
    let owner = Owner::Spec(elemental);
    let rage = ResourceDef {
        kind: ResourceKind::Rage,
        max: 100.0,
        initial: 0.0,
        regen_per_sec: 0.0,
        regen_hasted: false,
        recharge: None,
        out_of_combat: Some(-2.0),
    };
    let mut reported = |r: ResourceDef| {
        game.specs.get_mut(&elemental).unwrap().resources = vec![r];
        check_game_data(&game).contains(&DataIssue::InvalidOutOfCombat(owner.clone()))
    };
    assert!(!reported(rage));
    assert!(reported(ResourceDef {
        out_of_combat: Some(f64::NEG_INFINITY),
        ..rage
    }));
    assert!(reported(ResourceDef {
        recharge: Some(RechargeDef {
            concurrent: 1,
            period: SimDuration(1000),
            hasted: false,
        }),
        ..rage
    }));
}

#[test]
fn durations_per_unit_spent_need_a_duration_to_extend() {
    let mut game = game();
    let flame_shock = SpellId(188389);
    let owner = Owner::Spell(flame_shock);
    let mut reported = |per: SimDuration, effect_duration: Option<SimDuration>, timed: bool| {
        for e in &mut game.spells.get_mut(&flame_shock).unwrap().effects {
            if let Effect::ApplyAura {
                duration,
                per_unit_spent,
                ..
            } = e
            {
                *duration = effect_duration;
                *per_unit_spent = Some(per);
            }
        }
        let aura = game.auras.get_mut(&AuraId(188389)).unwrap();
        aura.duration = timed.then_some(SimDuration(18_000));
        check_game_data(&game).contains(&DataIssue::InvalidSpentDuration(owner.clone()))
    };
    let second = SimDuration(1000);
    assert!(!reported(second, None, true));
    assert!(!reported(second, Some(second), false));
    assert!(reported(second, None, false), "nothing to extend");
    assert!(reported(SimDuration::ZERO, None, true));
}

#[test]
fn banks_without_ticks_or_with_bad_fractions_are_reported() {
    let mut game = game();
    let flame_shock = AuraId(188389);
    let lava_surge = AuraId(77762);
    assert!(game.auras[&flame_shock].periodic.is_some());
    assert!(game.auras[&lava_surge].periodic.is_none());
    let bank = |draw| {
        Some(AuraValue {
            kind: AuraValueKind::Bank(draw),
            initial: None,
            cap: None,
            threshold: None,
            on_threshold: Vec::new(),
        })
    };
    let issues =
        |game: &GameData, aura| check_game_data(game).contains(&DataIssue::InvalidAuraValue(aura));
    for (draw, ok) in [
        (BankDraw::SpreadOverRemaining, true),
        (BankDraw::Fraction(1.0), true),
        (BankDraw::Fraction(0.0), false),
        (BankDraw::Fraction(1.5), false),
        (BankDraw::Fraction(f64::NAN), false),
    ] {
        game.auras.get_mut(&flame_shock).unwrap().value = bank(draw);
        assert_eq!(issues(&game, flame_shock), !ok, "{draw:?}");
    }
    game.auras.get_mut(&lava_surge).unwrap().value = bank(BankDraw::SpreadOverRemaining);
    assert!(issues(&game, lava_surge));
}

#[test]
fn broken_hero_trees_are_reported() {
    let mut game = game();
    let elemental = SpecId(262);
    let shared = TalentId(9000);
    let tree = |id: u32, keystone: u32| HeroTreeDef {
        id: HeroTreeId(id),
        name: format!("hero {id}"),
        specs: vec![elemental, SpecId(9999)],
        tree: TalentTree {
            name: format!("hero {id}"),
            points: 1,
            nodes: vec![TalentNode {
                row: 0,
                choices: vec![shared],
                max_rank: 1,
                requires_any: Vec::new(),
            }],
            gates: Vec::new(),
        },
        keystone: TalentId(keystone),
    };
    game.hero_trees = [tree(1, 9000), tree(2, 9999)]
        .into_iter()
        .map(|h| (h.id, h))
        .collect();
    let issues = check_game_data(&game);
    assert!(issues.contains(&DataIssue::UnknownSpec {
        owner: Owner::HeroTree(HeroTreeId(1)),
        spec: SpecId(9999),
    }));
    assert!(!issues.contains(&DataIssue::KeystoneNotInTree(HeroTreeId(1))));
    assert!(issues.contains(&DataIssue::KeystoneNotInTree(HeroTreeId(2))));
    assert!(issues.contains(&DataIssue::TalentInTwoTrees {
        spec: elemental,
        talent: shared,
    }));
}

#[test]
fn spawned_adds_must_be_adds_at_a_sensible_distance() {
    let game = game();
    let mut enemies = enemies();
    let dummy = EnemyKey("target_dummy".into());
    let mut add = enemies.enemies[&dummy].clone();
    add.key = EnemyKey("imp".into());
    add.kind = EnemyKind::Add;
    enemies.enemies.insert(add.key.clone(), add);
    let spawn = |adds: Vec<(&str, u32)>, distance: Option<Dist<f64>>| EnemyRule {
        name: EventName("summon".into()),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::SpawnAdds {
            adds: adds
                .into_iter()
                .map(|(k, n)| (EnemyKey(k.into()), n))
                .collect(),
            distance,
            despawn_with_spawner: false,
        },
    };
    let rules = &mut enemies.enemies.get_mut(&dummy).unwrap().rules;
    rules.push(spawn(vec![("imp", 2)], Some(Dist::Fixed(20.0))));
    assert_eq!(check_enemy_data(&enemies, &game), vec![]);

    let rules = &mut enemies.enemies.get_mut(&dummy).unwrap().rules;
    rules.push(spawn(vec![("imp", 0), ("target_dummy", 1)], None));
    rules.push(spawn(vec![("imp", 1)], Some(Dist::Fixed(-1.0))));
    let owner = Owner::Enemy(dummy.clone());
    assert_eq!(
        check_enemy_data(&enemies, &game),
        vec![
            DataIssue::InvalidAdds {
                owner: owner.clone(),
                enemy: EnemyKey("imp".into()),
            },
            DataIssue::InvalidAdds {
                owner: owner.clone(),
                enemy: dummy.clone(),
            },
            DataIssue::InvalidMovement(owner),
        ]
    );
}

#[test]
fn death_listeners_name_auras_that_exist() {
    let mut game = game();
    let ghost = AuraId(999_999);
    let holder = AuraId(77756);
    game.auras
        .get_mut(&holder)
        .expect("an aura to hold the listener")
        .listeners
        .push(Listener {
            on: ListenFor::EnemyDied {
                killed_by_self: false,
                had_aura: Some(ghost),
            },
            chance: ProcChance::Always,
            internal_cooldown: None,
            per_unit: false,
            shared_with: None,
            condition: None,
            effects: Vec::new(),
        });
    assert_eq!(
        check_game_data(&game),
        vec![DataIssue::UnknownAura {
            owner: Owner::Aura(holder),
            aura: ghost,
        }]
    );
}

#[test]
fn event_listeners_and_death_saves_must_be_able_to_happen() {
    let mut game = game();
    let ghost = AuraId(999_997);
    let holder = AuraId(77756);
    let def = game
        .auras
        .get_mut(&holder)
        .expect("an aura to hold the listeners");
    def.prevents_death = Some(PreventDeath {
        heal_to_pct: 1.5,
        lockout: Some(ghost),
        on_prevent: Vec::new(),
    });
    for on in [
        ListenFor::HealthBelow { pct: 0 },
        ListenFor::AuraStacksReached {
            aura: holder,
            stacks: 0,
        },
        ListenFor::Absorbed(ghost),
    ] {
        def.listeners.push(Listener {
            on,
            chance: ProcChance::Always,
            internal_cooldown: None,
            per_unit: false,
            shared_with: None,
            condition: None,
            effects: Vec::new(),
        });
    }
    let owner = Owner::Aura(holder);
    let unknown = DataIssue::UnknownAura {
        owner: owner.clone(),
        aura: ghost,
    };
    assert_eq!(
        check_game_data(&game),
        vec![
            DataIssue::InvalidDeathPrevention(holder),
            unknown.clone(),
            DataIssue::InvalidListener(owner.clone()),
            DataIssue::InvalidListener(owner),
            unknown,
        ]
    );
}

#[test]
fn target_sets_and_count_scales_name_auras_that_exist() {
    let mut game = game();
    let ghost = AuraId(999_998);
    let bolt = SpellId(188196);
    game.spells.get_mut(&bolt).unwrap().effects = vec![
        Effect::ForEach {
            target: EffectTarget::EnemiesWithAura {
                aura: ghost,
                from_self: true,
            },
            then: Vec::new(),
        },
        Effect::Damage {
            amount: Coefficient::Flat(1.0),
            school: SchoolMask::NATURE,
            target: EffectTarget::Target,
            aoe: None,
            ignores_armor: false,
            hand: None,
            per_count: Some(CountScale {
                count: TargetCount::EnemiesWithAura {
                    aura: ghost,
                    from_self: false,
                },
                pct: f64::NAN,
                form: CountForm::Linear,
            }),
            unmodified: false,
        },
        Effect::SpreadAura {
            aura: ghost,
            to: EffectTarget::AllEnemies,
            max: None,
        },
    ];
    let owner = Owner::Spell(bolt);
    let unknown = DataIssue::UnknownAura {
        owner: owner.clone(),
        aura: ghost,
    };
    assert_eq!(
        check_game_data(&game),
        vec![
            unknown.clone(),
            DataIssue::InvalidCountScale(owner),
            unknown.clone(),
            unknown
        ]
    );
}

#[test]
fn ground_auras_need_a_radius_and_standing_in_one_needs_one_on_the_ground() {
    let mut game = game();
    let holder = AuraId(77756);
    let def = game.auras.get_mut(&holder).expect("an aura to place");
    def.ground = Some(GroundDef { radius: 0.0 });
    def.modifiers.push(Modifier {
        scope: ModScope::All,
        kind: ModKind::DamageDonePct,
        value: 10.0,
        per_stack: false,
        condition: Some(Predicate::InOwnGround(holder)),
    });
    assert_eq!(
        check_game_data(&game),
        vec![DataIssue::InvalidGround(holder)],
        "a placed aura may be stood in"
    );
    game.auras.get_mut(&holder).unwrap().ground = None;
    assert_eq!(
        check_game_data(&game),
        vec![DataIssue::InvalidPredicate(Owner::Aura(holder))]
    );
}

#[test]
fn pets_can_only_run_dry_of_what_they_have_and_need_a_real_autocast_gap() {
    let mut game = game();
    let pet = game.pets.values_mut().next().expect("a pet to edit");
    let id = pet.id;
    pet.resources.clear();
    pet.expires_when_empty = Some(ResourceKind::Energy);
    assert_eq!(check_game_data(&game), vec![DataIssue::InvalidPet(id)]);
    let pet = game.pets.get_mut(&id).unwrap();
    pet.expires_when_empty = None;
    pet.autocast_interval = Some(SimDuration::ZERO);
    assert_eq!(check_game_data(&game), vec![DataIssue::InvalidPet(id)]);
    game.pets.get_mut(&id).unwrap().autocast_interval = Some(SimDuration(500));
    assert_eq!(check_game_data(&game), Vec::new());
}

#[test]
fn stuns_must_last_and_dispels_must_name_a_type_and_take_something() {
    let mut game = game();
    let bolt = SpellId(188196);
    let owner = Owner::Spell(bolt);
    let set = |game: &mut GameData, effect: Effect| {
        game.spells.get_mut(&bolt).expect("Lightning Bolt").effects = vec![effect];
    };
    let dispel = |types: Vec<DispelType>, count: u8| Effect::Dispel {
        types,
        count,
        target: EffectTarget::Target,
    };
    let invalid = vec![DataIssue::InvalidControl(owner)];
    set(
        &mut game,
        Effect::Stun {
            duration: SimDuration::ZERO,
            target: EffectTarget::Target,
        },
    );
    assert_eq!(check_game_data(&game), invalid);
    set(&mut game, dispel(Vec::new(), 1));
    assert_eq!(check_game_data(&game), invalid);
    set(&mut game, dispel(vec![DispelType::Magic], 0));
    assert_eq!(check_game_data(&game), invalid);
    set(&mut game, dispel(vec![DispelType::Magic], 1));
    assert_eq!(check_game_data(&game), Vec::new());
    set(
        &mut game,
        Effect::Stun {
            duration: SimDuration(1000),
            target: EffectTarget::Target,
        },
    );
    assert_eq!(check_game_data(&game), Vec::new());
}

#[test]
fn a_class_can_only_grant_its_pull_aura_by_a_spell_that_exists() {
    let mut game = game();
    let shaman = game.classes.get_mut("Shaman").expect("Shaman");
    shaman.on_pull[0].spell = Some(SpellId(1));
    assert_eq!(
        check_game_data(&game),
        vec![DataIssue::UnknownSpell {
            owner: Owner::Class("Shaman".into()),
            spell: SpellId(1),
        }]
    );
}

#[test]
fn conversions_need_two_stats_pen_a_share_and_falloff_a_rate() {
    let mut game = game();
    let aura = AuraId(60188);
    let set = |game: &mut GameData, kind: ModKind, value: f64| {
        game.auras.get_mut(&aura).expect("aura").modifiers = vec![Modifier {
            scope: ModScope::All,
            kind,
            value,
            per_stack: false,
            condition: None,
        }];
    };
    let invalid = vec![DataIssue::InvalidModifier(aura)];
    let convert = |to| ModKind::StatFromStat {
        from: Stat::CritRating,
        to,
    };
    set(&mut game, convert(Stat::CritRating), 10.0);
    assert_eq!(check_game_data(&game), invalid);
    set(&mut game, convert(Stat::MasteryRating), 10.0);
    assert_eq!(check_game_data(&game), Vec::new());
    for pen in [0.0, 101.0, f64::NAN] {
        set(&mut game, ModKind::ArmorPenPct, pen);
        assert_eq!(check_game_data(&game), invalid, "{pen}");
    }
    set(&mut game, ModKind::ArmorPenPct, 100.0);
    assert_eq!(check_game_data(&game), Vec::new());

    fn speed(game: &mut GameData) -> &mut portunus_gamedata::stats::RatingCurve {
        game.curves.ratings.get_mut(&Stat::Speed).expect("speed")
    }
    for (per_rating, scale) in [(-0.1, 0.1), (0.0003, 0.0), (f64::INFINITY, 0.1)] {
        speed(&mut game).falloff = Some(RatingFalloff { per_rating, scale });
        assert_eq!(
            check_game_data(&game),
            vec![DataIssue::InvalidRatingCurve(Stat::Speed)],
            "{per_rating} {scale}"
        );
    }
    speed(&mut game).falloff = Some(RatingFalloff {
        per_rating: 0.0,
        scale: 1.0,
    });
    assert_eq!(check_game_data(&game), Vec::new());
}

#[test]
fn resurrections_need_a_health_share_and_stones_must_survive_death() {
    let mut game = game();
    let spell = *game.spells.keys().next().expect("a spell");
    let rez = |health_pct| Effect::Resurrect {
        target: EffectTarget::Target,
        health_pct,
    };
    for bad in [0.0, 1.5, f64::NAN] {
        let mut g = game.clone();
        g.spells
            .get_mut(&spell)
            .expect("spell")
            .effects
            .push(rez(bad));
        assert_eq!(
            check_game_data(&g),
            vec![DataIssue::InvalidResurrect(Owner::Spell(spell))],
            "{bad}"
        );
    }
    game.spells
        .get_mut(&spell)
        .expect("spell")
        .effects
        .push(rez(1.0));
    assert_eq!(check_game_data(&game), Vec::new());

    let aura = AuraId(60188);
    game.auras.get_mut(&aura).expect("aura").self_resurrect = Some(0.6);
    assert_eq!(
        check_game_data(&game),
        vec![DataIssue::InvalidResurrect(Owner::Aura(aura))]
    );
    game.auras
        .get_mut(&aura)
        .expect("aura")
        .persists_through_death = true;
    assert_eq!(check_game_data(&game), Vec::new());
}
