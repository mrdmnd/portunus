//! SimC's item tables resolve a profile's gear to its real effects.
//!
//! These read SimC's checkout under `target/simc` and pass vacuously when
//! it is absent.

use std::path::PathBuf;

use portunus_core::{ItemId, SpellId};
use portunus_gamedata::item::GearSlot;
use portunus_ingest::{GearEffect, GearSource, ItemTables, ItemTrigger, SimcProfile, SpellDump};

fn simc_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/simc")
}

fn tables() -> Option<ItemTables> {
    let dir = simc_dir().join("engine/dbc/generated");
    dir.exists()
        .then(|| ItemTables::from_dir(&dir).expect("item tables"))
}

fn spells(effects: &[GearEffect]) -> Vec<(GearSource, SpellId, ItemTrigger)> {
    effects
        .iter()
        .filter_map(|e| match *e {
            GearEffect::Spell {
                source,
                spell,
                trigger,
                ..
            } => Some((source, spell, trigger)),
            GearEffect::Stat { .. } => None,
        })
        .collect()
}

#[test]
fn subtlety_gear_resolves_to_its_item_enchant_and_embellishment_spells() {
    let Some(t) = tables() else {
        return;
    };
    let profile =
        SimcProfile::from_file(&simc_dir().join("profiles/MID2/MID2_Rogue_Subtlety.simc"))
            .expect("profile");
    let slot = |s: GearSlot| t.equipped_effects(&profile.slots[&s]);

    assert_eq!(
        spells(&slot(GearSlot::Trinket2)),
        vec![(
            GearSource::Item(ItemId(270173)),
            SpellId(1291728),
            ItemTrigger::OnEquip
        )]
    );
    let heart = spells(&slot(GearSlot::Trinket1));
    assert!(heart.contains(&(
        GearSource::Item(ItemId(270175)),
        SpellId(1297761),
        ItemTrigger::OnUse
    )));
    assert!(spells(&slot(GearSlot::MainHand)).contains(&(
        GearSource::Enchant(8011),
        SpellId(1236730),
        ItemTrigger::OnEquip
    )));
    assert!(spells(&slot(GearSlot::Back)).contains(&(
        GearSource::Bonus(12384),
        SpellId(1283697),
        ItemTrigger::OnEquip
    )));
    assert!(spells(&slot(GearSlot::OffHand)).contains(&(
        GearSource::Bonus(13771),
        SpellId(1297382),
        ItemTrigger::OnEquip
    )));
    assert!(spells(&slot(GearSlot::Finger1)).contains(&(
        GearSource::Enchant(7967),
        SpellId(1236701),
        ItemTrigger::OnEquip
    )));
    // The head's diamond is a primary-stat gem; its enchant a stat and a spell.
    let head = slot(GearSlot::Head);
    assert!(head.iter().any(|e| matches!(
        e,
        GearEffect::Stat {
            source: GearSource::Gem(ItemId(240983)),
            item_mod: 71,
            ..
        }
    )));
    assert!(spells(&head).contains(&(
        GearSource::Enchant(8017),
        SpellId(1242773),
        ItemTrigger::OnEquip
    )));
    assert_eq!(t.embellishments[&12384].name, "Arcanoweave Lining");
    assert!(t.prop_points[&344].epic[0] > 300.0);
}

#[test]
fn item_spells_load_with_their_scaling() {
    let dir = simc_dir().join("SpellDataDump");
    if !dir.join("nonclass.txt").exists() {
        return;
    }
    let dump = SpellDump::nonclass_from_dir(&dir).expect("nonclass.txt");
    let tenacity = &dump.spells[&SpellId(1236730)];
    let linked = &tenacity.effects[0];
    assert_eq!(linked.scaling_class, Some(-7));
    assert_eq!(linked.trigger_spell, Some(SpellId(1241727)));
    assert!((linked.scaling_coefficient.unwrap() - 0.632558).abs() < 1e-6);
    assert_eq!(
        dump.spells[&SpellId(1241764)].effects[0].rating.as_deref(),
        Some("Vers")
    );
    assert!(dump.spells[&SpellId(1291728)].rppm.is_some());
    assert!(dump.spells[&SpellId(1241784)].scales_with_item_level);
}
