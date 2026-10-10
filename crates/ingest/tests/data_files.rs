//! The checked-in data files load, cross-check, compile, and sample.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{AuraId, HeroTreeId, Seed, SpecId, SpellId, TalentId};
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
