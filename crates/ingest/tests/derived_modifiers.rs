//! `data/derived_modifiers.ron` holds the damage modifiers the spell dump
//! gives each Rogue aura; `game.ron` keeps only the scripted ones.

use std::path::{Path, PathBuf};

use portunus_core::AuraId;
use portunus_gamedata::effect::Modifier;
use portunus_gamedata::GameData;
use portunus_ingest::modifiers::{derive_classes, derived_owns, DerivedFile};
use portunus_ingest::{read_ron, SpellDump};

const CLASSES: [&str; 1] = ["Rogue"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dump() -> Option<SpellDump> {
    let dir = root().join("target/simc/SpellDataDump");
    dir.is_dir()
        .then(|| SpellDump::from_dir(&dir).expect("spell dump"))
}

fn render(file: &DerivedFile) -> String {
    let config = ron::ser::PrettyConfig::new()
        .indentor("    ".to_owned())
        .depth_limit(4);
    let mut text = ron::ser::to_string_pretty(file, config).expect("serialize derived modifiers");
    text.push('\n');
    text
}

fn derive(dump: &SpellDump) -> DerivedFile {
    let data: GameData = read_ron(&root().join("data/game.ron")).expect("game.ron");
    derive_classes(&data, dump, &CLASSES)
}

#[test]
fn checked_in_file_matches_the_dump() {
    let Some(dump) = dump() else {
        eprintln!("no SpellDataDump under target/simc: skipped");
        return;
    };
    let path = root().join("data/derived_modifiers.ron");
    let checked_in = std::fs::read_to_string(&path).expect("derived_modifiers.ron");
    assert!(
        checked_in == render(&derive(&dump)),
        "{} is stale: run `cargo test -p portunus-ingest --test derived_modifiers -- --ignored`",
        path.display()
    );
}

/// The `game.ron` text without the modifier lines of `file`'s auras the
/// dump owns, and how many it dropped.
fn strip_owned(text: &str, file: &DerivedFile) -> (String, usize) {
    let mut out = String::with_capacity(text.len());
    let (mut in_auras, mut in_modifiers) = (false, false);
    let mut aura: Option<AuraId> = None;
    let mut dropped = 0;
    for line in text.lines() {
        match line {
            "    auras: {" => in_auras = true,
            "    }," => in_auras = false,
            "            modifiers: [" => in_modifiers = true,
            "            ]," => in_modifiers = false,
            _ => {}
        }
        if in_auras {
            if let Some(id) = line
                .strip_prefix("        ")
                .and_then(|l| l.strip_suffix(": ("))
                .and_then(|n| n.parse().ok())
            {
                aura = Some(AuraId(id));
            }
        }
        let entry = aura.and_then(|a| file.auras.get(&a)).filter(|_| in_auras);
        if let (Some(entry), true) = (entry, in_modifiers) {
            let item = line.trim().trim_end_matches(',');
            match ron::from_str::<Modifier>(item) {
                Ok(m) if derived_owns(&m, &entry.modifiers) => {
                    dropped += 1;
                    continue;
                }
                Ok(_) => {}
                Err(_) if item.starts_with('(') => {
                    eprintln!("{}: unparsed modifier {item}", entry.name)
                }
                Err(_) => {}
            }
        } else if let Some(entry) = entry.filter(|_| line.contains("modifiers: [(")) {
            eprintln!("{}: inline modifiers left as written", entry.name);
        }
        out.push_str(line);
        out.push('\n');
    }
    (out, dropped)
}

/// Writes `data/derived_modifiers.ron` and drops the `game.ron` modifiers
/// it supersedes.
#[test]
#[ignore]
fn regenerate() {
    let dump = dump().expect("SpellDataDump under target/simc");
    let file = derive(&dump);
    let game: &Path = &root().join("data/game.ron");
    let text = std::fs::read_to_string(game).expect("game.ron");
    let (stripped, dropped) = strip_owned(&text, &file);
    std::fs::write(game, stripped).expect("write game.ron");
    std::fs::write(root().join("data/derived_modifiers.ron"), render(&file))
        .expect("write derived_modifiers.ron");
    eprintln!(
        "{} auras derived, {dropped} game.ron modifiers dropped",
        file.auras.len()
    );
}
