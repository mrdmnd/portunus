//! Programmatic `SpellDataDump` synchronization and SimulationCraft (`simc`)
//! Patchwerk parity verification.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use portunus_core::{EnemyKey, HeroTreeId, Seat, SpecId};
use portunus_eval::{Estimate, LocalRunner, Metric, Runner, SeedSet};
use portunus_gamedata::stats::Stat;
use portunus_ingest::{sync_game_data_with_dump, DumpDiff, IngestError, SpellDump};
use portunus_loadout::{Compiler, LoadoutCompiler};

use crate::arm::Breakdown;
use crate::bundle::{Bundle, RunError};

/// Per-ability comparison between Portunus and SimulationCraft on a matched
/// Patchwerk fight.
#[derive(Debug, Clone)]
pub struct AbilityParityRow {
    pub name: String,
    pub portunus_casts: f64,
    pub simc_casts: f64,
    pub portunus_dps: f64,
    pub simc_dps: f64,
    pub portunus_share: f64,
    pub simc_share: f64,
}

/// Complete report comparing a [`Bundle`] against the local `simc` CLI binary.
#[derive(Debug, Clone)]
pub struct SimcParityReport {
    pub run_name: String,
    pub spec_id: SpecId,
    pub spec_name: String,
    pub hero_tree_name: Option<String>,
    pub simc_profile: PathBuf,
    pub dump_diffs_applied: usize,
    pub portunus_dps: Estimate,
    pub portunus_combat_secs: f64,
    pub simc_dps: f64,
    pub dps_ratio: f64,
    pub rows: Vec<AbilityParityRow>,
}

impl Bundle {
    /// Synchronizes this bundle's [`GameData`](portunus_gamedata::GameData) in-place against the
    /// `SpellDataDump` directory at `dump_dir` and recompiles all seat
    /// [`ActorTemplate`](portunus_loadout::ActorTemplate)s.
    pub fn sync_with_dump(&mut self, dump_dir: &Path) -> Result<Vec<DumpDiff>, RunError> {
        let dump = SpellDump::from_dir(dump_dir).map_err(|source| RunError::Read {
            path: dump_dir.to_owned(),
            source,
        })?;
        let data_mut = Arc::make_mut(&mut self.data);
        let diffs = sync_game_data_with_dump(data_mut, &dump, &BTreeSet::new());
        Arc::make_mut(&mut self.enemies).build = self.data.build.clone();
        for (seat, tpl) in self.templates.iter_mut().enumerate() {
            let loadout = tpl.source.clone();
            *tpl = Compiler
                .compile(&self.data, &loadout)
                .map_err(|source| RunError::Loadout { seat, source })?;
        }
        Ok(diffs)
    }

    /// Configures `target_dummy` for a true Patchwerk comparison against SimC:
    /// - Sets boss armor to `3167.14` (`30.0%` physical mitigation against
    ///   `armor_constant = 7390.0`, matching SimC's level-93 boss dummy).
    /// - Scales `target_dummy` health to `10,000,000` when the scenario is
    ///   `target_dummy.ron` so combat lasts ~100-150s instead of an 8-12s opener.
    pub fn configure_patchwerk_dummy(&mut self) {
        let key = EnemyKey("target_dummy".into());
        let is_short_dummy = self
            .config
            .scenario
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s == "target_dummy.ron");
        if let Some(dummy) = Arc::make_mut(&mut self.enemies).enemies.get_mut(&key) {
            if dummy.defense.armor <= 0.0 {
                // 3167.142857 / (3167.142857 + 7390.0) == 0.30 (30% boss armor mitigation).
                dummy.defense.armor = self.data.curves.armor_constant * 3.0 / 7.0;
            }
            if is_short_dummy && dummy.health <= 1_000_000 {
                dummy.health = 10_000_000;
            }
        }
    }
}

/// Locates the default SimulationCraft `MID2` (or `MID1`) profile for a given
/// [`SpecId`] and optional [`HeroTreeId`] under `simc_root`.
pub fn default_simc_profile(
    spec: SpecId,
    hero: Option<HeroTreeId>,
    simc_root: &Path,
) -> Option<PathBuf> {
    let mid2 = simc_root.join("profiles/MID2");
    let mid1 = simc_root.join("profiles/MID1");
    let candidates: &[&str] = match (spec.0, hero.map(|h| h.0)) {
        (71, _) => &["MID2_Warrior_Arms.simc"],
        (72, _) => &["MID2_Warrior_Fury.simc"],
        (262, Some(53)) => &[
            "MID2_Shaman_Elemental_Farseer.simc",
            "MID2_Shaman_Elemental.simc",
        ],
        (262, _) => &[
            "MID2_Shaman_Elemental_Stormbringer.simc",
            "MID2_Shaman_Elemental.simc",
        ],
        (263, Some(54)) => &[
            "MID2_Shaman_Enhancement_Stormbringer.simc",
            "MID2_Shaman_Enhancement_Totemic.simc",
        ],
        (263, _) => &[
            "MID2_Shaman_Enhancement_Totemic.simc",
            "MID2_Shaman_Enhancement_Stormbringer.simc",
        ],
        (70, Some(50)) => &[
            "MID2_Paladin_Retribution_Herald_of_the_Sun.simc",
            "MID2_Paladin_Retribution.simc",
        ],
        (70, _) => &[
            "MID2_Paladin_Retribution_Templar.simc",
            "MID2_Paladin_Retribution.simc",
        ],
        (251, _) => &["MID2_Death_Knight_Frost.simc"],
        (252, _) => &["MID2_Death_Knight_Unholy.simc"],
        (253, _) => &["MID2_Hunter_Beast_Mastery.simc"],
        (254, _) => &["MID2_Hunter_Marksmanship.simc"],
        (255, _) => &["MID2_Hunter_Survival.simc"],
        (259, _) => &["MID2_Rogue_Assassination.simc"],
        (260, _) => &["MID2_Rogue_Outlaw.simc"],
        (261, _) => &["MID2_Rogue_Subtlety.simc"],
        (62, _) => &["MID2_Mage_Arcane.simc"],
        (63, _) => &[
            "MID2_Mage_Fire_Sunfury.simc",
            "MID2_Mage_Fire_Frostfire.simc",
        ],
        (64, _) => &[
            "MID2_Mage_Frost_Spellslinger.simc",
            "MID2_Mage_Frost_Frostfire.simc",
        ],
        (265, _) => &["MID2_Warlock_Affliction.simc"],
        (266, _) => &["MID2_Warlock_Demonology.simc"],
        (267, _) => &["MID2_Warlock_Destruction.simc"],
        (258, _) => &[
            "MID2_Priest_Shadow_Archon.simc",
            "MID2_Priest_Shadow_Voidweaver.simc",
            "MID2_Priest_Shadow.simc",
        ],
        (269, _) => &["MID2_Monk_Windwalker.simc"],
        (102, _) => &[
            "MID2_Druid_Balance_Elunes_Chosen.simc",
            "MID2_Druid_Balance_Keeper_of_the_Grove.simc",
        ],
        (103, _) => &["MID2_Druid_Feral.simc"],
        (577, _) => &["MID2_Demon_Hunter_Havoc.simc"],
        (1480, _) => &["MID2_Demon_Hunter_Devourer.simc"],
        (1467, _) => &[
            "MID2_Evoker_Devastation.simc",
            "MID1_Evoker_Devastation.simc",
        ],
        (1473, _) => &[
            "MID2_Evoker_Augmentation.simc",
            "MID1_Evoker_Augmentation.simc",
        ],
        _ => &[],
    };
    for name in candidates {
        let p2 = mid2.join(name);
        if p2.exists() {
            return Some(p2);
        }
        let p1 = mid1.join(name.replace("MID2_", "MID1_"));
        if p1.exists() {
            return Some(p1);
        }
    }
    None
}

/// Runs both Portunus and the local `simc` CLI binary on a matched Patchwerk
/// setup (identical derived stats, weapon damage/speed, enemy armor, and zero external
/// buffs/consumables/trinkets/set-bonuses) and returns a [`SimcParityReport`].
pub fn check_simc_parity(
    bundle: &Bundle,
    runner: &LocalRunner,
    simc_bin: &Path,
    simc_profile: &Path,
    seeds: SeedSet,
    dump_diffs_applied: usize,
) -> Result<SimcParityReport, RunError> {
    let mut eval_bundle = bundle.clone();
    eval_bundle.configure_patchwerk_dummy();

    let seat_id = Seat(0);
    let tpl = &eval_bundle.templates[0];
    let spec_id = tpl.spec;
    let spec_def = &eval_bundle.data.specs[&spec_id];
    let hero_tree_name = tpl
        .hero_tree
        .and_then(|h| eval_bundle.data.hero_trees.get(&h))
        .map(|h| h.name.clone());

    let exp = eval_bundle.experiment(seeds);
    let rep = runner.run(&exp);
    let portunus_dps = rep.arms[0]
        .metrics
        .iter()
        .find(|(m, _)| *m == Metric::SeatDps(seat_id))
        .map(|(_, e)| *e)
        .unwrap_or(Estimate {
            n: 0,
            mean: 0.0,
            std_err: 0.0,
            ci95: (0.0, 0.0),
        });
    let bd = eval_bundle.breakdown(seeds).unwrap_or(Breakdown {
        runs: 0,
        combat_secs: 0.0,
        rows: Vec::new(),
    });

    // Build matched SimC CLI arguments from ActorTemplate.
    // Convert Portunus secondary ratings (700/660/700/780 per 1%) into SimC 12.1
    // level-90 ratings (46/44/46/54 per 1%) so both engines have the exact same
    // crit_pct, haste_pct, mastery_pct, and versatility_pct.
    let stat_val = |s: Stat| tpl.stats.0.get(&s).copied().unwrap_or(0.0);
    let (prim_abbrev, prim_total) = match spec_def.primary_stat {
        Stat::Strength => ("str", stat_val(Stat::Strength).round() as i64),
        Stat::Agility => ("agi", stat_val(Stat::Agility).round() as i64),
        _ => ("int", stat_val(Stat::Intellect).round() as i64),
    };
    // Level 90 base primary stat in SimC is ~440 (plus 5% armor specialization).
    let gear_prim = ((prim_total as f64 / 1.05).round() as i64 - 440).max(100);
    let crit = (stat_val(Stat::CritRating) * 46.0 / 700.0).round() as i64;
    let haste = (stat_val(Stat::HasteRating) * 44.0 / 660.0).round() as i64;
    let mast = (stat_val(Stat::MasteryRating) * 46.0 / 700.0).round() as i64;
    let vers = (stat_val(Stat::VersatilityRating) * 54.0 / 780.0).round() as i64;

    let chest_arg = format!(
        "chest=custom_chest,stats={gear_prim}{prim_abbrev}_4000sta_{crit}crit_{haste}haste_{mast}mastery_{vers}vers"
    );

    let mh_arg = if let Some(mh) = &tpl.main_hand {
        let wtype = if mh.ranged {
            "bow"
        } else if mh.speed.millis() >= 3000 {
            "sword2h"
        } else if mh.speed.millis() <= 2000 {
            "dagger"
        } else {
            "sword1h"
        };
        let spd = f64::from(mh.speed.millis()) / 1000.0;
        format!(
            "main_hand=custom_mh,weapon={wtype}_{spd:.2}speed_{:.0}min_{:.0}max",
            mh.min_damage, mh.max_damage
        )
    } else {
        "main_hand=custom_mh,weapon=staff_3.60speed_100min_150max".into()
    };

    let oh_arg = if let Some(oh) = &tpl.off_hand {
        let wtype = if oh.speed.millis() >= 3000 {
            "sword2h"
        } else if oh.speed.millis() <= 2000 {
            "dagger"
        } else {
            "sword1h"
        };
        let spd = f64::from(oh.speed.millis()) / 1000.0;
        format!(
            "off_hand=custom_oh,weapon={wtype}_{spd:.2}speed_{:.0}min_{:.0}max",
            oh.min_damage, oh.max_damage
        )
    } else {
        "off_hand=".into()
    };

    let target_secs = bd.combat_secs.round().clamp(15.0, 600.0) as u32;
    let iters = seeds.count.clamp(10, 50);

    let output = Command::new(simc_bin)
        .arg(simc_profile)
        .arg("potion=disabled")
        .arg("flask=disabled")
        .arg("food=disabled")
        .arg("augmentation=disabled")
        .arg("temporary_enchant=")
        .arg("omnium_talents=")
        .arg("disable_set_bonuses=1")
        .arg("optimal_raid=0")
        .arg("override.bloodlust=0")
        .arg("vary_combat_length=0.0")
        .arg("fixed_time=1")
        .arg(format!("max_time={target_secs}"))
        .arg(format!("iterations={iters}"))
        .arg("head=")
        .arg("neck=")
        .arg("shoulders=")
        .arg("back=")
        .arg("wrists=")
        .arg("hands=")
        .arg("waist=")
        .arg("legs=")
        .arg("feet=")
        .arg("finger1=")
        .arg("finger2=")
        .arg("trinket1=")
        .arg("trinket2=")
        .arg(chest_arg)
        .arg(mh_arg)
        .arg(oh_arg)
        .output()
        .map_err(|e| RunError::Read {
            path: simc_bin.to_owned(),
            source: IngestError::Io(e.to_string()),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let (simc_dps, simc_actions) = parse_simc_stdout(&stdout);

    let rows = merge_ability_rows(&bd, &simc_actions, simc_dps);
    let dps_ratio = if simc_dps > 0.0 {
        portunus_dps.mean / simc_dps
    } else {
        0.0
    };

    Ok(SimcParityReport {
        run_name: eval_bundle.config.name.clone(),
        spec_id,
        spec_name: spec_def.name.clone(),
        hero_tree_name,
        simc_profile: simc_profile.to_owned(),
        dump_diffs_applied,
        portunus_dps,
        portunus_combat_secs: bd.combat_secs,
        simc_dps,
        dps_ratio,
        rows,
    })
}

#[derive(Debug, Clone, Default)]
struct SimcActionRow {
    count: f64,
    dps: f64,
    share: f64,
}

fn parse_simc_stdout(stdout: &str) -> (f64, BTreeMap<String, SimcActionRow>) {
    let mut total_dps = 0.0;
    let mut actions: BTreeMap<String, SimcActionRow> = BTreeMap::new();
    let mut in_player_actions = false;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if total_dps == 0.0 && trimmed.starts_with("DPS=") {
            if let Some(val_str) = trimmed
                .strip_prefix("DPS=")
                .and_then(|s| s.split_whitespace().next())
            {
                total_dps = val_str.parse::<f64>().unwrap_or(0.0);
            }
        } else if line == "  Actions:" {
            in_player_actions = true;
        } else if in_player_actions {
            if line.starts_with("  Constant Buffs:")
                || line.starts_with("  Dynamic Buffs:")
                || line.starts_with("  Up-Times:")
                || line.starts_with("  Procs:")
                || line.starts_with("  Gains:")
                || line.starts_with("Target:")
                || line.starts_with("Player:")
            {
                in_player_actions = false;
                continue;
            }
            if line.starts_with("    ") && line.contains("Count=") && line.contains("pDPS=") {
                if let Some((raw_name, mut row)) = parse_simc_action_line(line) {
                    if row.share > 0.0 && total_dps > 0.0 {
                        row.dps = row.share * total_dps;
                    }
                    if row.dps > 0.0 {
                        let key = canonical_ability_key(&raw_name);
                        let entry = actions.entry(key).or_default();
                        entry.count += row.count;
                        entry.dps += row.dps;
                        entry.share += row.share;
                    }
                }
            }
        }
    }

    (total_dps, actions)
}

fn parse_simc_action_line(line: &str) -> Option<(String, SimcActionRow)> {
    let trimmed = line.trim();
    let count_pos = trimmed.find("Count=")?;
    let name = trimmed[..count_pos].trim().to_owned();
    let after_count = &trimmed[count_pos + 6..];
    let pipe_pos = after_count.find('|')?;
    let count = after_count[..pipe_pos].trim().parse::<f64>().unwrap_or(0.0);

    let dpe_pos = trimmed.find("DPE=")?;
    let after_dpe = &trimmed[dpe_pos + 4..];
    let dpe_pipe = after_dpe.find('|')?;
    let pct_str = after_dpe[dpe_pipe + 1..].split('%').next()?.trim();
    let share = pct_str.parse::<f64>().unwrap_or(0.0) / 100.0;

    let pdps_pos = trimmed.find("pDPS=")?;
    let after_pdps = &trimmed[pdps_pos + 5..];
    let pdps_tok = after_pdps.split_whitespace().next()?;
    let dps = pdps_tok.parse::<f64>().unwrap_or(0.0);

    Some((name, SimcActionRow { count, dps, share }))
}

fn canonical_ability_key(s: &str) -> String {
    let cleaned = s.replace(['\'', '\u{2019}'], "");
    let mut k: String = cleaned
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    while k.contains("__") {
        k = k.replace("__", "_");
    }
    let k = k.trim_matches('_').to_owned();
    match k.as_str() {
        "uncredited" | "auto_attack_mh" | "auto_attack_oh" | "melee" => "auto_attack".into(),
        "bladestorm_mh" | "bladestorm_oh" => "bladestorm".into(),
        "execute_damage" | "execute_off_hand" | "execute_mainhand" | "execute_offhand"
        | "execute_mh" | "execute_oh" => "execute".into(),
        "mortal_strike_bladestorm_unhinged" => "mortal_strike_unhinged".into(),
        "reap_the_storm_execute" => "reap_the_storm".into(),
        "rend_dot" => "rend".into(),
        "fatality" => "fatal_mark".into(),
        "charge_impact" => "charge".into(),
        "crushing_blow_mh" | "crushing_blow_oh" | "raging_blow_mh" | "raging_blow_oh" => {
            "raging_blow".into()
        }
        "rampage1" | "rampage2" | "rampage3" | "rampage4" | "rampage_1" | "rampage_2"
        | "rampage_3" | "rampage_4" => "rampage".into(),
        "bloodthirst_unhinged" | "bloodbath_bladestorm_unhinged" => "bloodthirst_unhinged".into(),
        "bloodbath_dot" | "bloodbath" => "bloodbath".into(),
        "odyns_fury_mh" | "odyns_fury_oh" => "odyns_fury".into(),
        "crusading_strike" | "crusading_strikes" => "crusading_strikes".into(),
        "divine_hammer_tick" | "divine_hammer" => "divine_hammer".into(),
        "execution_sentence_detonation" | "execution_sentence_init" | "execution_sentence" => {
            "execution_sentence".into()
        }
        "light_within_boj" | "light_within" => "light_within".into(),
        "seething_flames_0" | "seething_flames_1" | "seething_flames" => "seething_flames".into(),
        "consecration_tick_blade_of_justice" | "consecration_damage" | "consecration" => {
            "consecration".into()
        }
        "divine_storm_tempest" | "divine_storm" => "divine_storm".into(),
        "hammer_of_wrath_divine_toll" | "hammer_of_wrath_divine_resonance" => {
            "hammer_of_wrath_divine_toll".into()
        }
        "lava_burst_asc" | "lava_burst_ascendance" => "lava_burst".into(),
        "lava_burst_overload_asc" => "lava_burst_overload".into(),
        "elemental_blast_foe" | "elemental_blast_fusion_of_elements" => "elemental_blast".into(),
        "elemental_blast_overload_foe" => "elemental_blast_overload".into(),
        "flame_shock_secondary" => "flame_shock".into(),
        _ => {
            if let Some(rest) = k.strip_prefix("primal_fire_elemental_") {
                rest.to_owned()
            } else if let Some(rest) = k.strip_prefix("primal_storm_elemental_") {
                rest.to_owned()
            } else {
                k
            }
        }
    }
}

fn pretty_ability_name(key: &str, fallback: &str) -> String {
    match key {
        "auto_attack" => "Auto Attack".into(),
        "slayers_strike" => "Slayer's Strike".into(),
        "raging_blow" => "Raging Blow / Crushing Blow".into(),
        "rampage" => "Rampage".into(),
        "execute" => "Execute".into(),
        "bloodthirst_unhinged" => "Bloodthirst (Unhinged)".into(),
        "odyns_fury" => "Odyn's Fury".into(),
        "crusading_strikes" => "Crusading Strikes".into(),
        "divine_hammer" => "Divine Hammer".into(),
        "execution_sentence" => "Execution Sentence".into(),
        "light_within" => "Light Within".into(),
        "seething_flames" => "Seething Flames".into(),
        "consecration" => "Consecration".into(),
        "divine_storm" => "Divine Storm".into(),
        "hammer_of_wrath_divine_toll" => "Hammer of Wrath (Divine Toll)".into(),
        "highlords_judgment" => "Highlord's Judgment".into(),
        "truths_wake" => "Truth's Wake".into(),
        "elemental_blast" => "Elemental Blast".into(),
        "elemental_blast_overload" => "Elemental Blast Overload".into(),
        _ => fallback.to_owned(),
    }
}

fn merge_ability_rows(
    bd: &Breakdown,
    simc_actions: &BTreeMap<String, SimcActionRow>,
    simc_total_dps: f64,
) -> Vec<AbilityParityRow> {
    let mut merged: BTreeMap<String, AbilityParityRow> = BTreeMap::new();

    for r in &bd.rows {
        if r.dps <= 0.0 && r.casts <= 0.0 {
            continue;
        }
        let key = canonical_ability_key(&r.name);
        let entry = merged
            .entry(key.clone())
            .or_insert_with(|| AbilityParityRow {
                name: pretty_ability_name(&key, &r.name),
                portunus_casts: 0.0,
                simc_casts: 0.0,
                portunus_dps: 0.0,
                simc_dps: 0.0,
                portunus_share: 0.0,
                simc_share: 0.0,
            });
        if entry.portunus_casts == 0.0 {
            entry.portunus_casts = r.casts;
        } else if key != "rampage" && key != "raging_blow" && key != "execute" {
            entry.portunus_casts += r.casts;
        }
        entry.portunus_dps += r.dps;
        entry.portunus_share += r.share;
    }

    for (key, srow) in simc_actions {
        let entry = merged
            .entry(key.clone())
            .or_insert_with(|| AbilityParityRow {
                name: pretty_ability_name(key, key),
                portunus_casts: 0.0,
                simc_casts: 0.0,
                portunus_dps: 0.0,
                simc_dps: 0.0,
                portunus_share: 0.0,
                simc_share: 0.0,
            });
        entry.simc_casts += srow.count;
        entry.simc_dps += srow.dps;
        entry.simc_share += if simc_total_dps > 0.0 {
            srow.dps / simc_total_dps
        } else {
            srow.share
        };
    }

    let mut list: Vec<AbilityParityRow> = merged
        .into_values()
        .filter(|r| r.portunus_dps > 0.5 || r.simc_dps > 0.5)
        .collect();
    list.sort_by(|a, b| {
        let max_a = a.portunus_dps.max(a.simc_dps);
        let max_b = b.portunus_dps.max(b.simc_dps);
        max_b.total_cmp(&max_a)
    });
    list
}
