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
use portunus_gamedata::race::{EarthenGem, Loa, Race, TimeOfDay};
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

/// The same talents with the hero tree removed, for checks that the hero
/// nodes add auras without replacing the spec kit.
fn without_hero(data: &GameData, mut loadout: Loadout) -> Loadout {
    let Some(id) = loadout.hero_tree.take() else {
        return loadout;
    };
    let hero: std::collections::BTreeSet<TalentId> = data.hero_trees[&id]
        .tree
        .nodes
        .iter()
        .flat_map(|node| node.choices.iter().copied())
        .collect();
    loadout.talents.0.retain(|talent, _| !hero.contains(talent));
    loadout
}

#[test]
fn tables_have_no_dangling_references() {
    let game = game();
    let enemies = enemies();
    assert_eq!(check_game_data(&game), vec![]);
    assert_eq!(check_enemy_data(&enemies, &game), vec![]);
    assert_eq!(game.build, enemies.build);
}

/// Every playable race in the 12.1.5 racial table compiles, and the kit
/// follows the class: an orc warrior's Blood Fury is the attack-power
/// spell, an orc shaman's is the hybrid, an intellect undead uses Touch of
/// the Grave's spell-power coefficient.
#[test]
fn racials_follow_the_class_and_the_loadouts_choices() {
    let game = game();
    assert_eq!(game.races.len(), 26);

    let mut arms: Loadout =
        read_ron(&data_dir().join("loadouts/warrior_arms_slayer.ron")).expect("arms");
    let plain = Compiler.compile(&game, &arms).unwrap();
    assert!(!plain.abilities.contains(&SpellId(20572)));

    arms.race = Some(Race::Orc);
    let orc = Compiler.compile(&game, &arms).unwrap();
    assert!(orc.abilities.contains(&SpellId(20572)));
    assert!(!orc.abilities.contains(&SpellId(33702)));
    assert!(orc.passive_auras.contains(&AuraId(20573)));
    assert!(orc.passive_auras.contains(&AuraId(21563)));

    arms.race = Some(Race::Troll);
    let troll = Compiler.compile(&game, &arms).unwrap();
    assert!(troll.abilities.contains(&SpellId(26297)));
    let berserking = &game.auras[&AuraId(26297)];
    assert_eq!(berserking.duration, Some(SimDuration(12_000)));
    assert!(berserking
        .modifiers
        .iter()
        .any(|m| m.kind == ModKind::HastePct && m.value == 10.0));

    arms.race = Some(Race::BloodElf);
    let elf = Compiler.compile(&game, &arms).unwrap();
    assert!(elf.abilities.contains(&SpellId(69179)));
    assert!(elf.passive_auras.contains(&AuraId(154742)));

    let mut elemental: Loadout =
        read_ron(&data_dir().join("loadouts/shaman_elemental_stormbringer.ron"))
            .expect("elemental");
    elemental.race = Some(Race::Orc);
    let shaman = Compiler.compile(&game, &elemental).unwrap();
    assert!(shaman.abilities.contains(&SpellId(33697)));
    assert!(!shaman.abilities.contains(&SpellId(20572)));

    elemental.race = Some(Race::Undead);
    let undead = Compiler.compile(&game, &elemental).unwrap();
    assert!(undead.passive_auras.contains(&AuraId(910440)));
    assert!(!undead.passive_auras.contains(&AuraId(5227)));

    arms.race = Some(Race::NightElf);
    let day = Compiler.compile(&game, &arms).unwrap();
    assert!(day.passive_auras.contains(&AuraId(154748)));
    arms.elune = Some(TimeOfDay::Night);
    let night = Compiler.compile(&game, &arms).unwrap();
    assert!(night.passive_auras.contains(&AuraId(910500)));
    assert!(!night.passive_auras.contains(&AuraId(154748)));
    arms.elune = None;

    arms.race = Some(Race::ZandalariTroll);
    arms.loa = Some(Loa::Paku);
    let loa = Compiler.compile(&game, &arms).unwrap();
    assert!(loa.passive_auras.contains(&AuraId(292361)));
    assert!(loa.abilities.contains(&SpellId(291944)));
    arms.loa = Some(Loa::Kimbul);
    assert!(Compiler.compile(&game, &arms).is_err());
    arms.loa = None;

    arms.race = Some(Race::Earthen);
    arms.earthen_gem = Some(EarthenGem::Ruby);
    let earthen = Compiler.compile(&game, &arms).unwrap();
    assert!(earthen.abilities.contains(&SpellId(436344)));
    assert!(earthen.passive_auras.contains(&AuraId(910513)));
    arms.earthen_gem = None;

    arms.elune = Some(TimeOfDay::Day);
    assert!(Compiler.compile(&game, &arms).is_err());
    arms.elune = None;

    for race in game.races.keys().copied() {
        arms.race = Some(race);
        arms.loa = None;
        arms.elune = None;
        arms.earthen_gem = None;
        Compiler
            .compile(&game, &arms)
            .unwrap_or_else(|err| panic!("{race:?}: {err}"));
    }
}

#[test]
fn elemental_defaults_compile() {
    let game = game();
    let cases: &[(&str, HeroTreeId, &[u32], &[u32])] = &[
        (
            "loadouts/shaman_elemental_stormbringer.ron",
            HeroTreeId(54),
            &[8042, 51505, 188196, 188389, 191634],
            &[
                454009, 454021, 454026, 454391, 455096, 455110, 455129, 1264688, 1264691, 1264762,
            ],
        ),
        (
            "loadouts/shaman_elemental_farseer.ron",
            HeroTreeId(55),
            &[51505, 188196, 188389, 191634, 117014, 443454],
            &[
                443418, 443423, 443445, 443446, 443447, 443448, 443450, 443451, 448861, 1270446,
                1270447,
            ],
        ),
    ];
    for &(file, hero, spells, auras) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), vec![], "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, SpecId(262), "{file}");
        assert_eq!(template.hero_tree, Some(hero), "{file}");
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        for &aura in auras {
            assert!(
                template.passive_auras.contains(&AuraId(aura)),
                "{file}: {aura}"
            );
        }
        // 2000 base intellect plus 60% of the robe's 1000-point budget.
        assert!(
            (template.derived.spell_power - 2600.0).abs() < 1e-9,
            "{file}"
        );
    }
}

#[test]
fn arms_slayer_loadout_compiles() {
    let game = game();
    let loadout: Loadout =
        read_ron(&data_dir().join("loadouts/warrior_arms_slayer.ron")).expect("loadout");
    assert_eq!(Compiler.validate(&game, &loadout), vec![]);
    let template = Compiler.compile(&game, &loadout).expect("compiles");
    assert_eq!(template.hero_tree, Some(HeroTreeId(60)));
    // The keystone comes free; Brutal Finish and Opportunist lose their
    // choice nodes.
    for aura in [
        444767, 444769, 444772, 1270717, 444778, 444773, 1270718, 444771, 444775, 444779, 386628,
        444780,
    ] {
        assert!(template.passive_auras.contains(&AuraId(aura)), "{aura}");
    }
    let core = Compiler
        .compile(&game, &without_hero(&game, loadout))
        .expect("spec tree");
    assert_eq!(core.hero_tree, None);
    // Slayer adds no pressable spells. Massacre's Execute replaces the plain one.
    assert_eq!(template.abilities, core.abilities);
    for spell in [12294, 7384, 167105, 845, 227847, 107574, 281000, 1464, 100] {
        assert!(template.abilities.contains(&SpellId(spell)), "{spell}");
    }
    assert!(!template.abilities.contains(&SpellId(163201)));
    let held = |aura: u32| {
        core.passive_auras
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
fn arms_colossus_loadout_compiles() {
    let game = game();
    let loadout: Loadout =
        read_ron(&data_dir().join("loadouts/warrior_arms_colossus.ron")).expect("loadout");
    assert_eq!(Compiler.validate(&game, &loadout), vec![]);
    let template = Compiler.compile(&game, &loadout).expect("compiles");
    assert_eq!(template.hero_tree, Some(HeroTreeId(62)));
    assert!(template.abilities.contains(&SpellId(436358)));
    for aura in [
        436358, 429638, 440992, 1270704, 440995, 429641, 1270709, 429647, 431548, 429642, 1270710,
        429636,
    ] {
        assert!(template.passive_auras.contains(&AuraId(aura)), "{aura}");
    }
}

#[test]
fn fury_loadouts_compile() {
    let game = game();
    for (file, hero) in [
        ("warrior_fury_slayer.ron", Some(HeroTreeId(60))),
        ("warrior_fury_mountainthane.ron", Some(HeroTreeId(61))),
    ] {
        let loadout: Loadout = read_ron(&data_dir().join("loadouts").join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), vec![], "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.hero_tree, hero, "{file}");
        for spell in [
            23881, 85288, 184367, 280735, 190411, 1719, 385059, 227847, 107574, 6343,
        ] {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some() && template.off_hand.is_some());
    }
}

#[test]
fn enhancement_loadouts_compile() {
    let game = game();
    for (file, hero) in [
        ("shaman_enhancement_stormbringer.ron", Some(HeroTreeId(54))),
        ("shaman_enhancement_totemic.ron", Some(HeroTreeId(56))),
    ] {
        let loadout: Loadout = read_ron(&data_dir().join("loadouts").join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), vec![], "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.hero_tree, hero, "{file}");
        for spell in [17364, 60103, 188196, 188443, 188389, 470057, 187874] {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some() && template.off_hand.is_some());
    }
}

#[test]
fn retribution_loadouts_compile() {
    let game = game();
    for (file, hero) in [
        ("paladin_retribution_templar.ron", Some(HeroTreeId(48))),
        (
            "paladin_retribution_heraldofthesun.ron",
            Some(HeroTreeId(50)),
        ),
    ] {
        let loadout: Loadout = read_ron(&data_dir().join("loadouts").join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), vec![], "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.hero_tree, hero, "{file}");
        for spell in [
            35395, 20271, 24275, 184575, 383328, 53385, 255937, 343527, 375576,
        ] {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some());
    }
}

#[test]
fn frost_and_unholy_dk_loadouts_compile() {
    let game = game();
    for (file, hero) in [
        ("deathknight_frost_deathbringer.ron", Some(HeroTreeId(33))),
        ("deathknight_frost_rider.ron", Some(HeroTreeId(32))),
    ] {
        let loadout: Loadout = read_ron(&data_dir().join("loadouts").join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), vec![], "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.hero_tree, hero, "{file}");
        for spell in [
            49143, 49020, 49184, 47568, 207230, 51271, 279302, 1249658, 196770,
        ] {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some() && template.off_hand.is_some());
    }
    for (file, hero) in [
        ("deathknight_unholy_rider.ron", Some(HeroTreeId(32))),
        ("deathknight_unholy_sanlayn.ron", Some(HeroTreeId(31))),
    ] {
        let loadout: Loadout = read_ron(&data_dir().join("loadouts").join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), vec![], "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.hero_tree, hero, "{file}");
        for spell in [
            77575, 85948, 55090, 47541, 207317, 1247378, 1233448, 343294, 42650,
        ] {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some());
    }
}

#[test]
fn hunter_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/hunter_beastmastery_packleader.ron",
            SpecId(253),
            Some(43),
            &[34026, 217200, 193455, 19574],
        ),
        (
            "loadouts/hunter_beastmastery_darkranger.ron",
            SpecId(253),
            Some(44),
            &[34026, 217200, 193455, 19574, 466930],
        ),
        (
            "loadouts/hunter_marksmanship_sentinel.ron",
            SpecId(254),
            Some(42),
            &[19434, 257044, 212431, 53351, 288613, 260243],
        ),
        (
            "loadouts/hunter_marksmanship_darkranger.ron",
            SpecId(254),
            Some(44),
            &[19434, 257044, 212431, 466930, 288613, 260243],
        ),
        (
            "loadouts/hunter_survival_packleader.ron",
            SpecId(255),
            Some(43),
            &[259489, 259495, 186270, 1261193, 1250646],
        ),
        (
            "loadouts/hunter_survival_sentinel.ron",
            SpecId(255),
            Some(42),
            &[259489, 259495, 186270, 1261193, 1250646],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some());
    }
}

#[test]
fn rogue_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/rogue_assassination_deathstalker.ron",
            SpecId(259),
            Some(53),
            &[1329, 703, 1943, 32645, 360194, 385627],
        ),
        (
            "loadouts/rogue_assassination_fatebound.ron",
            SpecId(259),
            Some(52),
            &[1329, 703, 1943, 32645, 360194, 385627],
        ),
        (
            "loadouts/rogue_outlaw_trickster.ron",
            SpecId(260),
            Some(51),
            &[193315, 185763, 315341, 2098, 13750, 51690, 441776],
        ),
        (
            "loadouts/rogue_outlaw_fatebound.ron",
            SpecId(260),
            Some(52),
            &[193315, 185763, 315341, 2098, 13750, 51690],
        ),
        (
            "loadouts/rogue_subtlety_deathstalker.ron",
            SpecId(261),
            Some(53),
            &[
                53, 185438, 197835, 196819, 319175, 280719, 185313, 121471, 426591,
            ],
        ),
        (
            "loadouts/rogue_subtlety_trickster.ron",
            SpecId(261),
            Some(51),
            &[
                53, 185438, 197835, 196819, 319175, 280719, 185313, 121471, 426591, 441776,
            ],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some() && template.off_hand.is_some());
    }
}

#[test]
fn mage_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/mage_arcane_spellslinger.ron",
            SpecId(62),
            Some(40),
            &[30451, 44425, 5143, 153626, 321507, 365350, 12051, 1295924],
        ),
        (
            "loadouts/mage_arcane_sunfury.ron",
            SpecId(62),
            Some(39),
            &[30451, 44425, 5143, 153626, 321507, 365350, 12051, 1295924],
        ),
        (
            "loadouts/mage_fire_sunfury.ron",
            SpecId(63),
            Some(39),
            &[133, 108853, 11366, 2120, 2948, 153561, 190319],
        ),
        (
            "loadouts/mage_fire_frostfire.ron",
            SpecId(63),
            Some(41),
            &[431044, 108853, 11366, 2120, 2948, 153561, 190319],
        ),
        (
            "loadouts/mage_frost_spellslinger.ron",
            SpecId(64),
            Some(40),
            &[116, 30455, 44614, 84714, 190356, 205021, 1247777, 199786],
        ),
        (
            "loadouts/mage_frost_frostfire.ron",
            SpecId(64),
            Some(41),
            &[431044, 30455, 44614, 84714, 190356, 205021, 1247777, 199786],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some());
    }
}

#[test]
fn warlock_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/warlock_affliction_hellcaller.ron",
            SpecId(265),
            Some(58),
            &[
                980, 445468, 1259790, 27243, 48181, 198590, 1257052, 205180, 442726,
            ],
        ),
        (
            "loadouts/warlock_affliction_soulharvester.ron",
            SpecId(265),
            Some(57),
            &[980, 172, 1259790, 27243, 48181, 198590, 1257052, 205180],
        ),
        (
            "loadouts/warlock_demonology_diabolist.ron",
            SpecId(266),
            Some(59),
            &[
                686, 264178, 1250273, 104316, 264130, 265187, 1276452, 1276672, 434635, 434506,
            ],
        ),
        (
            "loadouts/warlock_demonology_soulharvester.ron",
            SpecId(266),
            Some(57),
            &[
                686, 264178, 1250273, 104316, 264130, 265187, 1276452, 1276672,
            ],
        ),
        (
            "loadouts/warlock_destruction_hellcaller.ron",
            SpecId(267),
            Some(58),
            &[
                29722, 445468, 17962, 116858, 5740, 17877, 152108, 1122, 6353, 442726,
            ],
        ),
        (
            "loadouts/warlock_destruction_diabolist.ron",
            SpecId(267),
            Some(59),
            &[
                29722, 348, 17962, 116858, 5740, 17877, 152108, 1122, 6353, 434635, 434506,
            ],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some());
    }
}

#[test]
fn priest_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/priest_shadow_voidweaver.ron",
            SpecId(258),
            Some(18),
            &[
                589, 34914, 8092, 15407, 32379, 335467, 1227280, 228260, 451234, 263165, 450405,
            ],
        ),
        (
            "loadouts/priest_shadow_archon.ron",
            SpecId(258),
            Some(19),
            &[
                589, 34914, 8092, 15407, 32379, 335467, 1227280, 228260, 1230339, 120644,
            ],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some());
    }
}

#[test]
fn monk_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/monk_windwalker_celestial.ron",
            SpecId(269),
            Some(64),
            &[
                100780, 100784, 107428, 101546, 322109, 113656, 1249625, 392983, 152175, 1250566,
                123904, 443028,
            ],
        ),
        (
            "loadouts/monk_windwalker_shadopan.ron",
            SpecId(269),
            Some(65),
            &[
                100780, 100784, 107428, 101546, 322109, 113656, 1249625, 392983, 152175, 1250566,
            ],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some() && template.off_hand.is_some());
    }
}

#[test]
fn druid_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/druid_balance_keeper.ron",
            SpecId(102),
            Some(23),
            &[
                190984, 194153, 8921, 93402, 78674, 191034, 1239669, 205636, 194223, 202770, 391528,
            ],
        ),
        (
            "loadouts/druid_balance_eluneschosen.ron",
            SpecId(102),
            Some(24),
            &[
                190984, 194153, 8921, 93402, 78674, 191034, 1239669, 205636, 194223, 202770, 391528,
            ],
        ),
        (
            "loadouts/druid_feral_druidoftheclaw.ron",
            SpecId(103),
            Some(21),
            &[
                5221, 1822, 106785, 106830, 1079, 22568, 441591, 5217, 106951, 1243807, 1244258,
            ],
        ),
        (
            "loadouts/druid_feral_wildstalker.ron",
            SpecId(103),
            Some(22),
            &[
                5221, 1822, 106785, 106830, 1079, 22568, 5217, 106951, 1243807, 1244258,
            ],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some());
    }
}

#[test]
fn demon_hunter_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/demonhunter_havoc_felscarred.ron",
            SpecId(577),
            Some(34),
            &[
                162794, 201427, 188499, 210152, 198013, 258920, 232893, 191427, 370965, 258860,
            ],
        ),
        (
            "loadouts/demonhunter_havoc_aldrachireaver.ron",
            SpecId(577),
            Some(35),
            &[
                162794, 201427, 188499, 210152, 198013, 258920, 232893, 191427, 370965, 258860,
                442294,
            ],
        ),
        (
            "loadouts/demonhunter_devourer_annihilator.ron",
            SpecId(1480),
            Some(124),
            &[
                473662, 1217605, 1226019, 1226025, 1215602, 473728, 1241937, 1221167, 1239519,
                1226033,
            ],
        ),
        (
            "loadouts/demonhunter_devourer_voidscarred.ron",
            SpecId(1480),
            Some(126),
            &[
                473662, 1217605, 1226019, 1226025, 1215602, 473728, 1241937, 1221167, 1239519,
                1226033,
            ],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some() && template.off_hand.is_some());
    }
}

#[test]
fn evoker_loadouts_compile() {
    let game = game();
    let cases: &[(&str, SpecId, Option<u32>, &[u32])] = &[
        (
            "loadouts/evoker_devastation_scalecommander.ron",
            SpecId(1467),
            Some(36),
            &[
                361469, 362969, 356995, 357211, 357208, 359073, 375087, 1265867, 357210,
            ],
        ),
        (
            "loadouts/evoker_devastation_flameshaper.ron",
            SpecId(1467),
            Some(37),
            &[
                361469, 362969, 356995, 357211, 357208, 359073, 375087, 1265867, 357210,
            ],
        ),
        (
            "loadouts/evoker_augmentation_scalecommander.ron",
            SpecId(1473),
            Some(36),
            &[
                361469, 362969, 357208, 395152, 395160, 396286, 403631, 360827, 409311, 404977,
            ],
        ),
        (
            "loadouts/evoker_augmentation_chronowarden.ron",
            SpecId(1473),
            Some(38),
            &[
                361469, 362969, 357208, 395152, 395160, 396286, 403631, 360827, 409311, 404977,
            ],
        ),
    ];
    for &(file, spec, hero, spells) in cases {
        let loadout: Loadout = read_ron(&data_dir().join(file)).expect(file);
        assert_eq!(Compiler.validate(&game, &loadout), Vec::new(), "{file}");
        let template = Compiler.compile(&game, &loadout).expect(file);
        assert_eq!(template.spec, spec);
        assert_eq!(template.hero_tree, hero.map(HeroTreeId));
        for &spell in spells {
            assert!(
                template.abilities.contains(&SpellId(spell)),
                "{file}: {spell}"
            );
        }
        assert!(template.main_hand.is_some());
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
fn channels_without_ticks_or_duration_are_reported() {
    let mut game = game();
    let channel = |duration: u32, ticks: u8| CastKind::Channel {
        duration: SimDuration(duration),
        ticks,
        hasted: true,
        swings: false,
        tick_zero: false,
        cost_per_tick: Vec::new(),
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
        cost_per_tick: Vec::new(),
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
            chain: false,
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
            chain: false,
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
            chain: false,
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
            refresh: false,
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
    set(&mut game, ModKind::CooldownRatePct, -100.0);
    assert_eq!(check_game_data(&game), invalid);
    set(&mut game, ModKind::TickPeriodPct, -150.0);
    assert_eq!(check_game_data(&game), invalid);
    set(&mut game, ModKind::CooldownRatePct, 50.0);
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

    game.spells.get_mut(&spell).expect("spell").on_cast = vec![
        Effect::TickAura {
            aura,
            target: EffectTarget::Target,
            fraction: 1.0,
        },
        Effect::ConsumeStacks {
            aura,
            target: EffectTarget::Target,
            max: Some(0),
            up_to_deficit: None,
            then: Vec::new(),
        },
    ];
    assert_eq!(
        check_game_data(&game),
        vec![
            DataIssue::InvalidControl(Owner::Spell(spell)),
            DataIssue::InvalidControl(Owner::Spell(spell)),
        ]
    );
}

#[test]
fn spell_dump_sync_validates_all_specs_and_loadouts() {
    use portunus_ingest::{sync_game_data_with_dump, SpellDump};
    use std::collections::BTreeSet;

    let dump_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/simc/SpellDataDump");
    if !dump_dir.exists() {
        return;
    }
    let dump = SpellDump::from_dir(&dump_dir).expect("loads SpellDataDump");
    assert!(dump.spells.len() > 2_000, "spells={}", dump.spells.len());

    let mut game = game();
    let diffs = sync_game_data_with_dump(&mut game, &dump, &BTreeSet::new());
    assert!(!diffs.is_empty(), "expected dump diffs");
    assert_eq!(check_game_data(&game), Vec::new());

    for entry in std::fs::read_dir(data_dir().join("loadouts")).expect("loadouts dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("ron") {
            continue;
        }
        let loadout: Loadout = read_ron(&path).expect("read loadout");
        assert_eq!(
            Compiler.validate(&game, &loadout),
            Vec::new(),
            "loadout {:?}",
            path
        );
        let _ = Compiler.compile(&game, &loadout).expect("compile loadout");
    }
}

#[test]
fn talent_auditor_flags_known_bug_patterns_and_subtlety_is_clean() {
    use portunus_gamedata::effect::CooldownChange;
    use portunus_ingest::{audit_talents, SpellDump, TalentFindingKind};
    use std::collections::BTreeMap;

    let dump_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/simc/SpellDataDump");
    let dump = dump_dir
        .exists()
        .then(|| SpellDump::from_dir(&dump_dir).expect("loads SpellDataDump"));

    let game = game();
    let findings = audit_talents(&game, dump.as_ref());

    let mut by_kind: BTreeMap<TalentFindingKind, usize> = BTreeMap::new();
    let mut by_spec: BTreeMap<String, usize> = BTreeMap::new();
    for f in &findings {
        *by_kind.entry(f.kind).or_default() += 1;
        *by_spec.entry(f.spec_name.clone()).or_default() += 1;
        let desc = dump
            .as_ref()
            .and_then(|d| d.spells.get(&SpellId(f.talent_id.0)))
            .map(|s| s.description.replace('\n', " ").replace('\r', ""))
            .unwrap_or_else(|| "<no dump entry>".into());
        println!(
            "  [{:?}] {} | {} ({}) -> aura {} | desc: {}",
            f.kind, f.spec_name, f.talent_name, f.talent_id.0, f.aura_or_spell_id, desc
        );
    }
    println!(
        "[talent-audit] total findings across 26 specs: {} ({:?})",
        findings.len(),
        by_kind
    );
    println!("[talent-audit] findings by spec: {by_spec:?}");

    // Every spec and hero tree across all 26 specs must have zero
    // UnconsumedPassiveOverride, DeprecatedCooldownReduction, MissingDumpCooldownOrRppm,
    // or InertOffensiveTalent findings.
    assert!(
        findings.is_empty(),
        "All 26 specs should have zero talent audit findings across all 4 categories: {findings:#?}"
    );

    // Inject the 3 exact historical Subtlety bugs into a mutated copy and verify
    // `audit_talents` catches all 3:
    // 1. `The First Dance` (382505) checked directly as a permanent passive on `Shadow Dance` (185313)
    //    without removing a ready buff.
    // 2. `Deepening Shadows` (185314) having a deprecated cooldown-reduction listener on `185313`.
    // 3. `Backstab` (53) triggering `Unseen Blade` (441144) on every cast without checking `459485`.
    let mut buggy = game.clone();
    buggy.spells.get_mut(&SpellId(185313)).unwrap().effects[0] = Effect::If {
        when: Predicate::CasterHasAura(AuraId(382505)),
        then: vec![Effect::ApplyAura {
            aura: AuraId(185422),
            target: EffectTarget::Caster,
            stacks: 1,
            duration: Some(SimDuration(10000)),
            per_unit_spent: None,
        }],
        otherwise: vec![Effect::ApplyAura {
            aura: AuraId(185422),
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
            per_unit_spent: None,
        }],
    };
    buggy.auras.get_mut(&AuraId(185314)).unwrap().listeners = vec![Listener {
        on: ListenFor::ResourceSpent(ResourceKind::ComboPoints),
        per_unit: false,
        condition: None,
        chance: ProcChance::Always,
        internal_cooldown: None,
        shared_with: None,
        chain: false,
        effects: vec![Effect::AdjustCooldown {
            spell: SpellId(185313),
            change: CooldownChange::Reduce(SimDuration(700)),
        }],
    }];
    buggy
        .spells
        .get_mut(&SpellId(53))
        .unwrap()
        .effects
        .push(Effect::If {
            when: Predicate::CasterHasAura(AuraId(441146)),
            then: vec![Effect::TriggerSpell {
                spell: SpellId(441144),
                target: EffectTarget::Target,
                delay: SimDuration::ZERO,
            }],
            otherwise: Vec::new(),
        });
    buggy
        .auras
        .get_mut(&AuraId(1250359))
        .unwrap()
        .modifiers
        .clear();

    let buggy_findings = audit_talents(&buggy, dump.as_ref());
    assert!(
        buggy_findings.iter().any(|f| {
            f.talent_id == TalentId(1250359) && f.kind == TalentFindingKind::InertOffensiveTalent
        }),
        "must flag Canny Strikes when its modifiers are stripped"
    );
    assert!(
        buggy_findings.iter().any(|f| {
            f.talent_id == TalentId(382505)
                && f.kind == TalentFindingKind::UnconsumedPassiveOverride
        }),
        "must flag The First Dance permanent-passive override bug"
    );
    if dump.is_some() {
        assert!(
            buggy_findings.iter().any(|f| {
                f.talent_id == TalentId(185314)
                    && f.kind == TalentFindingKind::DeprecatedCooldownReduction
            }),
            "must flag Deepening Shadows deprecated CDR bug"
        );
        assert!(
            buggy_findings.iter().any(|f| {
                f.talent_id == TalentId(441146)
                    && f.kind == TalentFindingKind::MissingDumpCooldownOrRppm
            }),
            "must flag Unseen Blade missing cooldown lockout bug"
        );
    }
}
