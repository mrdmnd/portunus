//! In-game loadout strings decode as SimC reads them, and encode back.
//!
//! These read SimC's checkout under `target/simc` and pass vacuously when
//! it is absent.

use std::path::PathBuf;

use portunus_core::SpellId;
use portunus_ingest::{DecodedTalents, SimcProfile, TraitTable, TraitTree};

fn simc_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/simc")
}

fn table() -> Option<TraitTable> {
    let path = simc_dir().join("engine/dbc/generated/trait_data.inc");
    path.exists()
        .then(|| TraitTable::from_file(&path).expect("trait_data.inc"))
}

fn profile_talents(name: &str) -> String {
    SimcProfile::from_file(&simc_dir().join("profiles/MID2").join(name))
        .expect("profile")
        .talents
        .expect("talents=")
}

fn names(d: &DecodedTalents, tree: TraitTree) -> Vec<&str> {
    let mut v: Vec<&str> = d
        .picks
        .iter()
        .filter(|p| p.tree == tree)
        .map(|p| p.name.as_str())
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn every_mid2_profile_decodes_and_encodes_back() {
    let Some(table) = table() else { return };
    let mut seen = 0;
    for entry in std::fs::read_dir(simc_dir().join("profiles/MID2")).expect("MID2") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "simc") {
            continue;
        }
        let Some(talents) = SimcProfile::from_file(&path).expect("profile").talents else {
            continue;
        };
        let decoded = table
            .decode(&talents)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            decoded.hero_sub_tree.is_some(),
            "{} selects a hero tree",
            path.display()
        );
        let again = table
            .decode(&table.encode(&decoded).expect("encode"))
            .expect("decode the encoding");
        assert_eq!(again, decoded, "{}", path.display());
        seen += 1;
    }
    assert!(seen >= 30, "only {seen} MID2 profiles had talents");
}

#[test]
fn subtlety_reads_as_simc_reads_it() {
    let Some(table) = table() else { return };
    let d = table
        .decode(&profile_talents("MID2_Rogue_Subtlety.simc"))
        .expect("decode");
    assert_eq!(d.spec.0, 261);
    assert_eq!(d.hero_sub_tree, Some(53), "Deathstalker");
    // As SimC's `debug=1` lists them, less the class tree.
    assert_eq!(
        names(&d, TraitTree::Spec),
        [
            "Ancient Arts",
            "Dark Brew",
            "Dark Shadow",
            "Death Perception",
            "Deepening Shadows",
            "Deeper Daggers",
            "Double Dance",
            "Exhilarating Execution",
            "Fade to Nothing",
            "Finality",
            "Find Weakness",
            "Goremaw's Bite",
            "Improved Backstab",
            "Improved Find Weakness",
            "Improved Shuriken Storm",
            "Lingering Shadow",
            "Master of Shadows",
            "Premeditation",
            "Relentless Strikes",
            "Replicating Shadows",
            "Secret Stratagem",
            "Shadow Blades",
            "Shadow Focus",
            "Shadowcraft",
            "Shadowed Finishers",
            "Silent Storm",
            "Umbral Edge",
            "Veiltouched",
            "Warning Signs",
        ]
    );
    // The string also holds a Trickster pick the game ignores.
    let hero = names(&d, TraitTree::Hero);
    assert!(hero.contains(&"Deathstalker's Mark"));
    assert!(!hero.contains(&"Unseen Blade"));
    assert_eq!(hero.len(), 14);
    let finality = d
        .picks
        .iter()
        .find(|p| p.name == "Finality")
        .expect("Finality");
    assert_eq!(finality.rank, 2);
    // A tiered node is one pick: its first entry, all its ranks (1 + 2 + 1).
    let arts = d
        .picks
        .iter()
        .find(|p| p.name == "Ancient Arts")
        .expect("Ancient Arts");
    assert_eq!((arts.spell, arts.rank), (SpellId(1268932), 4));
}

#[test]
fn a_hero_tree_swaps_in_through_an_encoding() {
    let Some(table) = table() else { return };
    let sub = table
        .decode(&profile_talents("MID2_Rogue_Subtlety.simc"))
        .expect("decode");
    let outlaw = table
        .decode(&profile_talents("MID2_Rogue_Outlaw.simc"))
        .expect("decode");
    assert_eq!(outlaw.hero_sub_tree, Some(51), "Trickster");
    let mut swapped = sub.clone();
    swapped.picks.retain(|p| p.tree != TraitTree::Hero);
    swapped.picks.extend(
        outlaw
            .picks
            .iter()
            .filter(|p| p.tree == TraitTree::Hero)
            .cloned(),
    );
    swapped.hero_sub_tree = outlaw.hero_sub_tree;
    let d = table
        .decode(&table.encode(&swapped).expect("encode"))
        .expect("decode");
    assert_eq!(d.hero_sub_tree, Some(51));
    assert_eq!(names(&d, TraitTree::Spec), names(&sub, TraitTree::Spec));
    assert_eq!(names(&d, TraitTree::Hero), names(&outlaw, TraitTree::Hero));
}
