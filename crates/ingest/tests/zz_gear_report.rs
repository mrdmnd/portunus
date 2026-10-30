use std::path::PathBuf;

use portunus_ingest::{GearEffect, ItemTables, SimcProfile, SpellDump};

#[test]
#[ignore]
fn report() {
    let simc = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/simc");
    let t = ItemTables::from_dir(&simc.join("engine/dbc/generated")).unwrap();
    let dump = SpellDump::nonclass_from_dir(&simc.join("SpellDataDump")).unwrap();
    let profile = std::env::var("PROFILE").unwrap_or("MID2_Rogue_Subtlety.simc".into());
    let p = SimcProfile::from_file(&simc.join("profiles/MID2").join(profile)).unwrap();
    for (slot, s) in &p.slots {
        println!("== {slot:?} {} ilvl {}", s.name, s.item_level);
        for e in t.equipped_effects(s) {
            match e {
                GearEffect::Spell {
                    source,
                    spell,
                    trigger,
                    ..
                } => {
                    let d = dump.spells.get(&spell);
                    println!(
                        "  {source:?} {trigger:?} {} {:?} rppm={:?} dur={:?}",
                        spell.0,
                        d.map(|d| d.name.as_str()),
                        d.and_then(|d| d.rppm),
                        d.and_then(|d| d.duration_ms)
                    );
                    if let Some(d) = d {
                        for ef in &d.effects {
                            println!(
                                "     #{} avg={:.1} {} sv={:?} c={:?} cls={:?} trig={:?} r={:?} st={:?}",
                                ef.index,
                                t.effect_average(d, ef.index, Some(t.scaling_item(s))),
                                ef.kind.chars().take(70).collect::<String>(),
                                ef.scaled_value,
                                ef.scaling_coefficient,
                                ef.scaling_class,
                                ef.trigger_spell.map(|s| s.0),
                                ef.rating,
                                ef.stat
                            );
                        }
                    }
                }
                GearEffect::Stat {
                    source,
                    item_mod,
                    coefficient,
                    scaling_class,
                    amount,
                } => {
                    println!("  {source:?} stat mod {item_mod} coeff {coefficient} cls {scaling_class} amt {amount}");
                }
            }
        }
    }
}
