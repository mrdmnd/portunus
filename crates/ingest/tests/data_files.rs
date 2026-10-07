//! The checked-in data files load, cross-check, compile, and sample.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{AuraId, Seed, SpellId};
use portunus_gamedata::{EnemyData, GameData};
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
        [8042, 51505, 188196, 188389].map(SpellId).to_vec()
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
    assert_eq!(dummy.max_health, 400_000);
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
        owner: Owner::Spec(portunus_core::SpecId(262)),
        spell: SpellId(8042),
    }));
}
