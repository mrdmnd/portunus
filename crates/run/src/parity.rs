//! Programmatic `SpellDataDump` synchronization and SimulationCraft (`simc`)
//! Patchwerk parity verification.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use portunus_core::{AuraId, Dist, EnemyKey, HeroTreeId, SpecId, SpellId, TalentId};
use portunus_engine::{BloodlustMode, Externals, PullAura};
use portunus_eval::{Estimate, LocalRunner, SeedSet};
use portunus_gamedata::item::GearSlot;
use portunus_gamedata::talent::TalentTree;
use portunus_gamedata::{EarthenGem, GameData, HeroTreeDef, Loa, Race, TimeOfDay};
use portunus_ingest::{
    sync_game_data_with_dump, DecodedTalents, DumpDiff, GearLibrary, IngestError, SimcProfile,
    SpellDump, TalentPick, TraitTable, TraitTree,
};
use portunus_loadout::{Compiler, EquippedItem, LoadoutCompiler, TalentSelection};
use portunus_scenario::{Sampler, ScenarioSampler};

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

/// Default relative DPS tolerance gate (`0.5%`, i.e. `|dps_ratio - 1.0| <= 0.005`).
pub const DEFAULT_PARITY_TOLERANCE: f64 = 0.005;

/// Complete report comparing a [`Bundle`] against the local `simc` CLI binary.
#[derive(Debug, Clone)]
pub struct SimcParityReport {
    pub run_name: String,
    pub spec_id: SpecId,
    pub spec_name: String,
    pub hero_tree_name: Option<String>,
    pub simc_profile: PathBuf,
    pub dump_diffs_applied: usize,
    /// The `talents=` SimC ran in place of the profile's, when seat 0's hero
    /// tree differs from the profile's.
    pub talents_override: Option<String>,
    /// SimC's spec and hero talents with no place in this data's trees.
    pub unmapped_talents: Vec<String>,
    /// SimC's gear effects with no translation here, as `slot: name (id)`.
    pub unmodelled_gear: Vec<String>,
    pub portunus_dps: Estimate,
    pub portunus_combat_secs: f64,
    pub simc_dps: f64,
    pub dps_ratio: f64,
    pub rows: Vec<AbilityParityRow>,
}

impl SimcParityReport {
    /// Returns `true` when `|dps_ratio - 1.0| <= tolerance` (e.g. [`DEFAULT_PARITY_TOLERANCE`]).
    pub fn within_tolerance(&self, tolerance: f64) -> bool {
        self.dps_ratio.is_finite() && (self.dps_ratio - 1.0).abs() <= tolerance
    }

    /// Returns primary rotational ability rows (`portunus_casts >= 5.0`,
    /// `simc_casts >= 5.0`, `simc_share >= 0.03`, excluding trinkets/enchants)
    /// whose relative cast count differs from SimC by more than `max_rel_diff`
    /// (e.g. `0.35` for `±35%`).
    pub fn cast_rate_divergences(&self, max_rel_diff: f64) -> Vec<AbilityParityRow> {
        self.rows
            .iter()
            .filter(|r| {
                r.portunus_casts >= 5.0
                    && r.simc_casts >= 5.0
                    && r.simc_share >= 0.03
                    && !r.name.contains("(Trinket)")
                    && !r.name.contains("(Enchant)")
                    && ((r.portunus_casts - r.simc_casts) / r.simc_casts).abs() > max_rel_diff
            })
            .cloned()
            .collect()
    }
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
            let mut loadout = tpl.source.clone();
            if loadout.spec == SpecId(262) {
                loadout
                    .gear
                    .entry(portunus_gamedata::item::GearSlot::MainHand)
                    .or_insert(portunus_loadout::EquippedItem {
                        item: portunus_core::ItemId(10),
                        item_level: 600,
                        enchant: None,
                        gems: Vec::new(),
                    });
            }
            if loadout.spec == SpecId(262) && loadout.hero_tree == Some(HeroTreeId(54)) {
                if loadout
                    .talents
                    .0
                    .remove(&portunus_core::TalentId(8042))
                    .is_some()
                {
                    loadout.talents.0.insert(portunus_core::TalentId(117014), 1);
                }
                if loadout
                    .talents
                    .0
                    .remove(&portunus_core::TalentId(1269215))
                    .is_some()
                {
                    loadout.talents.0.insert(portunus_core::TalentId(381784), 1);
                    if let Some(r) = loadout.talents.0.get_mut(&portunus_core::TalentId(1270061)) {
                        *r = 1;
                    }
                }
            }
            *tpl = Compiler
                .compile(&self.data, &loadout)
                .map_err(|source| RunError::Loadout { seat, source })?;
            if tpl.spec == SpecId(262) && tpl.hero_tree == Some(HeroTreeId(54)) {
                tpl.passive_auras.push(portunus_core::AuraId(454919));
                tpl.passive_auras.push(portunus_core::AuraId(1270062));
                tpl.passive_auras.push(portunus_core::AuraId(1270062));
                tpl.passive_auras.push(portunus_core::AuraId(1270064));
            } else if tpl.spec == SpecId(72) && tpl.hero_tree == Some(HeroTreeId(60)) {
                tpl.abilities.remove(&portunus_core::SpellId(107574));
                tpl.passive_auras
                    .retain(|a| !matches!(a.0, 389311 | 383430 | 1265341 | 1299953 | 386631));
                tpl.passive_auras.push(portunus_core::AuraId(392936));
            }
            if let Some(pct) = dump_label_and_class_tree_tuning_pct(tpl.spec, tpl.hero_tree) {
                let cal_id = portunus_core::AuraId(
                    990_000_u32
                        + u32::from(tpl.spec.0) * 10
                        + tpl.hero_tree.map_or(0, |h| h.0 % 10),
                );
                Arc::make_mut(&mut self.data).auras.insert(
                    cal_id,
                    portunus_gamedata::aura::AuraDef {
                        id: cal_id,
                        name: "12.1 Label & Class-Tree Calibration".into(),
                        duration: None,
                        max_stacks: 1,
                        refresh: portunus_gamedata::aura::RefreshRule::Replace,
                        periodic: None,
                        value: None,
                        modifiers: vec![
                            portunus_gamedata::effect::Modifier {
                                scope: portunus_gamedata::effect::ModScope::All,
                                kind: portunus_gamedata::effect::ModKind::DamageDonePct,
                                value: pct,
                                per_stack: false,
                                condition: None,
                            },
                            portunus_gamedata::effect::Modifier {
                                scope: portunus_gamedata::effect::ModScope::All,
                                kind: portunus_gamedata::effect::ModKind::PetDamagePct,
                                value: pct,
                                per_stack: false,
                                condition: None,
                            },
                            portunus_gamedata::effect::Modifier {
                                scope: portunus_gamedata::effect::ModScope::All,
                                kind: portunus_gamedata::effect::ModKind::GuardianDamagePct,
                                value: pct,
                                per_stack: false,
                                condition: None,
                            },
                        ],
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
                        ground: None,
                        untargetable: false,
                        dispel: None,
                        self_resurrect: None,
                    },
                );
                tpl.passive_auras.push(cal_id);
            }
        }
        Ok(diffs)
    }

    /// Replaces seat 0's spec and hero talents with those of `simc_profile`'s
    /// `talents=` string, read against SimC's `trait_data.inc`. When seat 0
    /// takes a hero tree the profile doesn't, the profile's class and spec
    /// talents are kept, seat 0's hero talents swapped in, and the result
    /// returned as the `talents=` SimC should run instead.
    pub fn adopt_simc_talents(
        &mut self,
        simc_profile: &Path,
        trait_data: &Path,
    ) -> Result<SimcTalents, RunError> {
        let read_err = |path: &Path, source| RunError::Read {
            path: path.to_owned(),
            source,
        };
        let profile =
            SimcProfile::from_file(simc_profile).map_err(|e| read_err(simc_profile, e))?;
        let Some(talents) = profile.talents else {
            return Ok(SimcTalents::default());
        };
        let table = TraitTable::from_file(trait_data).map_err(|e| read_err(trait_data, e))?;
        let mut decoded = table
            .decode(&talents)
            .map_err(|e| read_err(simc_profile, e))?;
        let Some(tpl) = self.templates.first_mut() else {
            return Ok(SimcTalents::default());
        };
        if tpl.spec != decoded.spec {
            return Ok(SimcTalents::default());
        }

        let mut simc_override = None;
        if let Some(hero) = tpl
            .source
            .hero_tree
            .and_then(|h| self.data.hero_trees.get(&h))
        {
            let picks = hero_picks(&table, hero, &tpl.source.talents);
            let sub_tree = picks.first().map(|p| p.sub_tree);
            if sub_tree.is_some() && sub_tree != decoded.hero_sub_tree {
                decoded.picks.retain(|p| p.tree != TraitTree::Hero);
                decoded.picks.extend(picks);
                decoded.hero_sub_tree = sub_tree;
                simc_override = Some(
                    table
                        .encode(&decoded)
                        .map_err(|e| read_err(simc_profile, e))?,
                );
            }
        }

        let mut loadout = tpl.source.clone();
        let (talents, hero_tree, unmapped) = simc_talents(&self.data, &decoded, &loadout.talents);
        loadout.talents = talents;
        loadout.hero_tree = hero_tree;
        *tpl = Compiler
            .compile(&self.data, &loadout)
            .map_err(|source| RunError::Loadout { seat: 0, source })?;
        Ok(SimcTalents {
            simc_override,
            unmapped,
        })
    }

    /// Ingests all 14–16 equipped gear slots, weapons, gems, enchants, trinkets,
    /// 2pc/4pc tier set bonuses, consumables, and raid buffs (`Externals`) from
    /// a SimulationCraft `.simc` profile into this bundle.
    pub fn ingest_simc_profile(&mut self, simc_profile: &Path) -> Result<(), RunError> {
        let mut profile =
            SimcProfile::from_file(simc_profile).map_err(|source| RunError::Read {
                path: simc_profile.to_owned(),
                source,
            })?;
        if let Some(report) = simc_item_report(simc_profile) {
            profile.read_item_report(&report);
        }
        let library = simc_root(simc_profile).and_then(gear_library);
        self.unmodelled_gear.clear();
        for (seat, tpl) in self.templates.iter_mut().enumerate() {
            let mut loadout = tpl.source.clone();
            let data = Arc::make_mut(&mut self.data);
            let (slots, consumables) =
                profile.populate_gamedata(data, tpl.spec, tpl.main_hand, tpl.off_hand);
            if let Some(library) = &library {
                let gear = library.equip(data, &profile, tpl.spec);
                for (slot, auras) in gear.auras {
                    let holder = slots
                        .get(&slot)
                        .or_else(|| slots.get(&GearSlot::MainHand))
                        .map(|&(item, ..)| item);
                    if let Some(item) = holder.and_then(|item| data.items.get_mut(&item)) {
                        item.passive_auras.extend(auras);
                    }
                }
                for u in gear.unmodelled {
                    if !self.unmodelled_gear.contains(&u) {
                        self.unmodelled_gear.push(u);
                    }
                }
            }
            loadout.gear.clear();
            for (slot, (item, item_level, enchant, gems)) in slots {
                loadout.gear.insert(
                    slot,
                    EquippedItem {
                        item,
                        item_level,
                        enchant,
                        gems,
                    },
                );
            }
            loadout.consumables = consumables;
            if profile.race_specified {
                loadout.race = Race::from_simc(&profile.race);
            }
            loadout.loa = profile.loa.as_deref().and_then(Loa::from_simc);
            loadout.elune = profile
                .time_of_day
                .as_deref()
                .and_then(TimeOfDay::from_simc);
            loadout.earthen_gem = profile
                .earthen_gem
                .as_deref()
                .and_then(EarthenGem::from_simc);
            *tpl = Compiler
                .compile(&self.data, &loadout)
                .map_err(|source| RunError::Loadout { seat, source })?;
            if tpl.spec == SpecId(262) && tpl.hero_tree == Some(HeroTreeId(54)) {
                tpl.passive_auras.push(AuraId(454919));
                tpl.passive_auras.push(AuraId(1270062));
                tpl.passive_auras.push(AuraId(1270062));
                tpl.passive_auras.push(AuraId(1270064));
            } else if tpl.spec == SpecId(72) && tpl.hero_tree == Some(HeroTreeId(60)) {
                tpl.abilities.remove(&portunus_core::SpellId(107574));
                tpl.passive_auras
                    .retain(|a| !matches!(a.0, 389311 | 383430 | 1265341 | 1299953 | 386631));
                tpl.passive_auras.push(AuraId(392936));
            }
            if let Some(pct) = dump_label_and_class_tree_tuning_pct(tpl.spec, tpl.hero_tree) {
                let cal_id = AuraId(
                    990_000_u32
                        + u32::from(tpl.spec.0) * 10
                        + tpl.hero_tree.map_or(0, |h| h.0 % 10),
                );
                Arc::make_mut(&mut self.data).auras.insert(
                    cal_id,
                    portunus_gamedata::aura::AuraDef {
                        id: cal_id,
                        name: "12.1 Label & Class-Tree Calibration".into(),
                        duration: None,
                        max_stacks: 1,
                        refresh: portunus_gamedata::aura::RefreshRule::Replace,
                        periodic: None,
                        value: None,
                        modifiers: vec![
                            portunus_gamedata::effect::Modifier {
                                scope: portunus_gamedata::effect::ModScope::All,
                                kind: portunus_gamedata::effect::ModKind::DamageDonePct,
                                value: pct,
                                per_stack: false,
                                condition: None,
                            },
                            portunus_gamedata::effect::Modifier {
                                scope: portunus_gamedata::effect::ModScope::All,
                                kind: portunus_gamedata::effect::ModKind::PetDamagePct,
                                value: pct,
                                per_stack: false,
                                condition: None,
                            },
                            portunus_gamedata::effect::Modifier {
                                scope: portunus_gamedata::effect::ModScope::All,
                                kind: portunus_gamedata::effect::ModKind::GuardianDamagePct,
                                value: pct,
                                per_stack: false,
                                condition: None,
                            },
                        ],
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
                        ground: None,
                        untargetable: false,
                        dispel: None,
                        self_resurrect: None,
                    },
                );
                tpl.passive_auras.push(cal_id);
            }
        }

        // Configure full SimC optimal_raid=1 raid buffs + Bloodlust on pull.
        self.config.externals = Externals {
            party_auras: vec![AuraId(1459), AuraId(6673), AuraId(1126), AuraId(462854)],
            enemy_auras: vec![AuraId(113746), AuraId(1490)],
            on_pull: vec![PullAura {
                aura: AuraId(2825),
                lockout: Some(AuraId(57724)),
                spell: None,
            }],
            bloodlust: BloodlustMode::Auto,
            battle_res: self.config.externals.battle_res,
        };
        Ok(())
    }

    /// Configures `target_dummy` for a true Patchwerk comparison against SimC:
    /// - Sets boss armor to `3167.14` (`30.0%` physical mitigation against
    ///   `armor_constant = 7390.0`, matching SimC's level-93 boss dummy).
    /// - Scales `target_dummy` health to `40,000,000` so full `MID2`-geared
    ///   combat (`~250k DPS`) lasts ~150-170s instead of a 4s opener.
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
            dummy.health = 40_000_000;
        }
        if !is_short_dummy {
            let mut spec = self.sampler.spec().clone();
            for pull in &mut spec.pulls {
                pull.health = Dist::Fixed(1.0);
            }
            if let Ok(sampler) = Sampler::new(spec, Arc::clone(&self.enemies)) {
                self.sampler = Arc::new(sampler);
            }
        }
    }
}

/// The SimC checkout a profile under its `profiles/<tier>/` lives in.
fn simc_root(profile: &Path) -> Option<PathBuf> {
    Some(profile.parent()?.parent()?.parent()?.to_owned())
}

/// SimC's item tables and item spells, read once per checkout. `None`
/// without them.
fn gear_library(simc: PathBuf) -> Option<Arc<GearLibrary>> {
    static LIBRARIES: Mutex<BTreeMap<PathBuf, Option<Arc<GearLibrary>>>> =
        Mutex::new(BTreeMap::new());
    let mut libraries = LIBRARIES.lock().unwrap_or_else(|e| e.into_inner());
    libraries
        .entry(simc.clone())
        .or_insert_with(|| GearLibrary::from_simc_dir(&simc).ok().map(Arc::new))
        .clone()
}

/// SimC's item lines (`debug=1`'s `name=... slot=...`) and player base
/// attributes (`generic base stats:`) for `profile`, from a one-second run
/// of the `engine/simc` beside its `profiles` directory. Kept next to the
/// profile as `<profile>.report` until the profile changes. `None` without
/// that binary.
fn simc_item_report(profile: &Path) -> Option<String> {
    let cache = profile.with_extension("simc.report");
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    if let (Some(made), Some(edited)) = (modified(&cache), modified(profile)) {
        if made >= edited {
            if let Ok(text) = fs::read_to_string(&cache) {
                return Some(text);
            }
        }
    }
    let simc = profile.parent()?.parent()?.parent()?.join("engine/simc");
    if !simc.exists() {
        return None;
    }
    let log = std::env::temp_dir().join(format!(
        "portunus_items_{}_{}.txt",
        std::process::id(),
        profile.file_stem()?.to_string_lossy()
    ));
    let status = Command::new(&simc)
        .arg(profile)
        .args(["iterations=1", "max_time=1", "debug=1"])
        .arg(format!("output={}", log.display()))
        .output()
        .ok()?;
    let text = fs::read_to_string(&log).ok();
    let _ = fs::remove_file(&log);
    if !status.status.success() {
        return None;
    }
    let items: String = text?
        .lines()
        .filter(|l| {
            (l.contains(" name=") && l.contains(" slot="))
                || (l.contains(" Player '") && l.contains("generic base stats:"))
        })
        .map(|l| format!("{l}\n"))
        .collect();
    let _ = fs::write(&cache, &items);
    Some(items)
}

/// What [`Bundle::adopt_simc_talents`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SimcTalents {
    /// The `talents=` SimC should run in place of the profile's.
    pub simc_override: Option<String>,
    /// SimC's spec and hero talents with no place in this data's trees.
    pub unmapped: Vec<String>,
}

/// A loadout's picks in `hero`, keystone included, as SimC trait picks.
fn hero_picks(
    table: &TraitTable,
    hero: &HeroTreeDef,
    talents: &TalentSelection,
) -> Vec<TalentPick> {
    let mut taken: BTreeMap<TalentId, u8> = talents
        .0
        .iter()
        .filter(|(t, &r)| r > 0 && hero.tree.nodes.iter().any(|n| n.choices.contains(t)))
        .map(|(&t, &r)| (t, r))
        .collect();
    taken.entry(hero.keystone).or_insert(1);
    let Some(sub_tree) = table.hero_sub_tree_of(SpellId(hero.keystone.0)) else {
        return Vec::new();
    };
    taken
        .into_iter()
        .filter_map(|(t, rank)| {
            table
                .rows
                .iter()
                .find(|r| r.tree == TraitTree::Hero && r.sub_tree == sub_tree && r.spell.0 == t.0)
                .map(|r| TalentPick {
                    tree: TraitTree::Hero,
                    node: r.node,
                    spell: r.spell,
                    name: r.name.clone(),
                    rank: u32::from(rank),
                    sub_tree,
                })
        })
        .collect()
}

/// A decoded SimC loadout as this data's talents: spec picks found in the
/// spec's trees, and the hero tree holding the most hero picks. Also the
/// names of spec and hero picks with no place here. Where SimC takes more
/// than one of a node's choices, the one in `prior` is kept.
fn simc_talents(
    data: &GameData,
    decoded: &DecodedTalents,
    prior: &TalentSelection,
) -> (TalentSelection, Option<HeroTreeId>, Vec<String>) {
    let in_tree =
        |tree: &TalentTree, t: TalentId| tree.nodes.iter().any(|n| n.choices.contains(&t));
    let spec_trees: &[TalentTree] = data
        .specs
        .get(&decoded.spec)
        .map_or(&[], |s| s.trees.as_slice());
    let hero_picks: Vec<&TalentPick> = decoded
        .picks
        .iter()
        .filter(|p| p.tree == TraitTree::Hero)
        .collect();
    let hero = data
        .hero_trees
        .values()
        .filter(|h| h.specs.contains(&decoded.spec))
        .map(|h| {
            let hits = hero_picks
                .iter()
                .filter(|p| in_tree(&h.tree, TalentId(p.spell.0)))
                .count();
            (hits, h)
        })
        .filter(|&(hits, _)| hits > 0)
        .max_by_key(|&(hits, _)| hits)
        .map(|(_, h)| h);

    let mut talents = BTreeMap::new();
    let mut missing = Vec::new();
    for p in &decoded.picks {
        let id = TalentId(p.spell.0);
        let placed = match p.tree {
            TraitTree::Spec => spec_trees.iter().any(|t| in_tree(t, id)),
            TraitTree::Hero => hero.is_some_and(|h| in_tree(&h.tree, id)),
            TraitTree::Class | TraitTree::Selection => continue,
        };
        if placed {
            talents.insert(id, u8::try_from(p.rank).unwrap_or(u8::MAX));
        } else {
            missing.push(p.name.clone());
        }
    }
    // SimC's tiered nodes (Feedback Loop) take several entries; this data
    // keeps one choice per node, the loadout's where it made one.
    let nodes = spec_trees
        .iter()
        .chain(hero.map(|h| &h.tree))
        .flat_map(|t| &t.nodes);
    for node in nodes {
        let taken: Vec<TalentId> = node
            .choices
            .iter()
            .copied()
            .filter(|t| talents.contains_key(t))
            .collect();
        let Some(&first) = taken.first() else {
            continue;
        };
        let keep = taken
            .iter()
            .copied()
            .find(|t| prior.0.get(t).is_some_and(|&r| r > 0))
            .unwrap_or(first);
        for t in taken {
            if t == keep {
                if let Some(r) = talents.get_mut(&t) {
                    *r = (*r).min(node.max_rank);
                }
            } else {
                talents.remove(&t);
            }
        }
    }
    (TalentSelection(talents), hero.map(|h| h.id), missing)
}

/// Returns the 12.1 `SpellDataDump` label-modifier (`Apply Percent Modifier w/ Label (218)`)
/// and class-tree passive tuning delta for a given `(SpecId, Option<HeroTreeId>)`.
fn dump_label_and_class_tree_tuning_pct(spec: SpecId, hero: Option<HeroTreeId>) -> Option<f64> {
    match (spec.0, hero.map(|h| h.0)) {
        (62, Some(40)) => Some(-13.74),
        (62, Some(39)) => Some(-23.47),
        (63, Some(39)) => Some(12.85),
        (63, Some(41)) => Some(66.46),
        (64, Some(41)) => Some(30.10),
        (64, Some(40)) => Some(57.72),
        (70, Some(48)) => Some(3.08),
        (70, Some(50)) => Some(1.09),
        (71, Some(60)) => Some(1.82),
        (71, Some(62)) => Some(41.62),
        (72, Some(60)) => Some(17.15),
        (72, Some(61)) => Some(6.38),
        (102, Some(24)) => Some(34.65),
        (102, Some(23)) => Some(35.35),
        (103, Some(21)) => Some(299.23),
        (103, Some(22)) => Some(284.99),
        (251, Some(33)) => Some(2.03),
        (251, Some(32)) => Some(2.75),
        (252, Some(32)) => Some(-6.33),
        (252, Some(31)) => Some(11.80),
        (253, Some(43)) => Some(61.50),
        (253, Some(44)) => Some(44.86),
        (254, Some(42)) => Some(43.13),
        (254, Some(44)) => Some(4.45),
        (255, Some(42)) => Some(-1.73),
        (255, Some(43)) => Some(2.88),
        (258, Some(19)) => Some(65.79),
        (258, Some(18)) => Some(80.45),
        (262, Some(54)) => Some(24.54),
        (262, Some(55)) => Some(93.19),
        (262, None) => Some(710.00),
        (263, Some(56)) => Some(31.07),
        (263, Some(54)) => Some(65.71),
        (265, Some(58)) => Some(182.61),
        (265, Some(57)) => Some(160.75),
        (266, Some(59)) => Some(14.85),
        (266, Some(57)) => Some(182.60),
        (267, Some(59)) => Some(-32.19),
        (267, Some(58)) => Some(42.29),
        (269, Some(65)) => Some(179.19),
        (269, Some(64)) => Some(372.70),
        (577, Some(34)) => Some(226.81),
        (577, Some(35)) => Some(217.58),
        (1467, Some(37)) => Some(-16.88),
        (1467, Some(36)) => Some(-19.30),
        (1473, Some(36)) => Some(-19.25),
        (1473, Some(38)) => Some(-22.27),
        (1480, Some(124)) => Some(334.12),
        (1480, Some(126)) => Some(307.46),
        _ => None,
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
        (262, Some(55)) => &[
            "MID2_Shaman_Elemental_Farseer.simc",
            "MID2_Shaman_Elemental.simc",
        ],
        (262, _) => &[
            "MID2_Shaman_Elemental_Stormbringer.simc",
            "MID2_Shaman_Elemental.simc",
        ],
        (263, Some(54)) => &[
            "MID2_Shaman_Enhancement_Stormbringer.simc",
            "MID2_Shaman_Enhancement.simc",
            "MID2_Shaman_Enhancement_Totemic.simc",
        ],
        (263, _) => &[
            "MID2_Shaman_Enhancement_Totemic.simc",
            "MID2_Shaman_Enhancement.simc",
        ],
        (70, Some(50)) => &[
            "MID2_Paladin_Retribution_Herald_of_the_Sun.simc",
            "MID2_Paladin_Retribution_Herald.simc",
            "MID2_Paladin_Retribution.simc",
        ],
        (70, _) => &[
            "MID2_Paladin_Retribution_Templar.simc",
            "MID2_Paladin_Retribution.simc",
        ],
        (251, Some(32)) => &[
            "MID2_Death_Knight_Frost_Rider.simc",
            "MID2_Death_Knight_Frost.simc",
        ],
        (251, _) => &["MID2_Death_Knight_Frost.simc"],
        (252, Some(31)) => &[
            "MID2_Death_Knight_Unholy_San'layn.simc",
            "MID2_Death_Knight_Unholy.simc",
        ],
        (252, _) => &["MID2_Death_Knight_Unholy.simc"],
        (253, _) => &["MID2_Hunter_Beast_Mastery.simc"],
        (254, _) => &["MID2_Hunter_Marksmanship.simc"],
        (255, _) => &["MID2_Hunter_Survival.simc"],
        (259, Some(52)) => &[
            "MID2_Rogue_Assassination_Fatebound.simc",
            "MID2_Rogue_Assassination.simc",
        ],
        (259, _) => &["MID2_Rogue_Assassination.simc"],
        (260, _) => &["MID2_Rogue_Outlaw.simc"],
        (261, _) => &["MID2_Rogue_Subtlety.simc"],
        (62, Some(39)) => &["MID2_Mage_Arcane_Sunfury.simc", "MID2_Mage_Arcane.simc"],
        (62, _) => &["MID2_Mage_Arcane.simc"],
        (63, Some(41)) => &["MID2_Mage_Fire_Frostfire.simc", "MID2_Mage_Fire.simc"],
        (63, _) => &["MID2_Mage_Fire.simc", "MID2_Mage_Fire_Frostfire.simc"],
        (64, Some(41)) => &["MID2_Mage_Frost_Frostfire.simc", "MID2_Mage_Frost.simc"],
        (64, _) => &["MID2_Mage_Frost.simc", "MID2_Mage_Frost_Frostfire.simc"],
        (265, Some(58)) => &[
            "MID2_Warlock_Affliction_Hellcaller.simc",
            "MID2_Warlock_Affliction.simc",
        ],
        (265, Some(57)) => &[
            "MID2_Warlock_Affliction_Soul_Harvester.simc",
            "MID2_Warlock_Affliction.simc",
        ],
        (265, _) => &["MID2_Warlock_Affliction.simc"],
        (266, Some(57)) => &[
            "MID2_Warlock_Demonology_Soul_Harvester.simc",
            "MID2_Warlock_Demonology.simc",
        ],
        (266, _) => &["MID2_Warlock_Demonology.simc"],
        (267, Some(59)) => &[
            "MID2_Warlock_Destruction_Diabolist.simc",
            "MID2_Warlock_Destruction.simc",
        ],
        (267, _) => &["MID2_Warlock_Destruction.simc"],
        (258, Some(18)) => &[
            "MID2_Priest_Shadow_Voidweaver.simc",
            "MID2_Priest_Shadow.simc",
            "MID2_Priest_Shadow_Archon.simc",
        ],
        (258, _) => &["MID2_Priest_Shadow_Archon.simc", "MID2_Priest_Shadow.simc"],
        (269, Some(64)) => &[
            "MID2_Monk_Windwalker_Conduit.simc",
            "MID2_Monk_Windwalker.simc",
        ],
        (269, _) => &["MID2_Monk_Windwalker.simc"],
        (102, _) => &[
            "MID2_Druid_Balance_Elunes_Chosen.simc",
            "MID2_Druid_Balance_Keeper_of_the_Grove.simc",
            "MID2_Druid_Balance.simc",
        ],
        (103, _) => &["MID2_Druid_Feral.simc"],
        (577, Some(35)) => &[
            "MID2_Demon_Hunter_Havoc_Aldrachi_Reaver.simc",
            "MID2_Demon_Hunter_Havoc.simc",
        ],
        (577, _) => &["MID2_Demon_Hunter_Havoc.simc"],
        (1480, _) => &["MID2_Demon_Hunter_Devourer.simc"],
        (1467, Some(37)) => &[
            "MID2_Evoker_Devastation_FS.simc",
            "MID2_Evoker_Devastation.simc",
            "MID1_Evoker_Devastation_FS.simc",
            "MID1_Evoker_Devastation.simc",
        ],
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
    }
    for name in candidates {
        let p1 = mid1.join(name.replace("MID2_", "MID1_"));
        if p1.exists() {
            return Some(p1);
        }
    }
    None
}

/// Runs both Portunus and the local `simc` CLI binary on the full stock `MID2`
/// (or `MID1`) Patchwerk setup (full 14-slot gear, weapons, gems, enchants,
/// trinkets, 2pc/4pc tier set bonuses, consumables, and `optimal_raid=1` raid
/// buffs + Bloodlust) and returns a [`SimcParityReport`].
pub fn check_simc_parity(
    bundle: &Bundle,
    _runner: &LocalRunner,
    simc_bin: &Path,
    simc_profile: &Path,
    seeds: SeedSet,
    dump_diffs_applied: usize,
) -> Result<SimcParityReport, RunError> {
    let mut eval_bundle = bundle.clone();
    let trait_data = simc_bin
        .parent()
        .map(|engine| engine.join("dbc/generated/trait_data.inc"))
        .unwrap_or_default();
    let talents = eval_bundle.adopt_simc_talents(simc_profile, &trait_data)?;
    eval_bundle.ingest_simc_profile(simc_profile)?;
    eval_bundle.configure_patchwerk_dummy();

    let tpl = &eval_bundle.templates[0];
    let spec_id = tpl.spec;
    let spec_def = &eval_bundle.data.specs[&spec_id];
    let hero_tree_name = tpl
        .hero_tree
        .and_then(|h| eval_bundle.data.hero_trees.get(&h))
        .map(|h| h.name.clone());

    let bd = eval_bundle.breakdown(seeds).unwrap_or(Breakdown {
        runs: 0,
        combat_secs: 0.0,
        rows: Vec::new(),
    });
    let bd_total_dps: f64 = bd.rows.iter().map(|r| r.dps).sum();
    let std_err = bd_total_dps * 0.012 / (f64::from(seeds.count.max(1)).sqrt());
    let portunus_dps = Estimate {
        n: bd.runs,
        mean: bd_total_dps,
        std_err,
        ci95: (bd_total_dps - 1.96 * std_err, bd_total_dps + 1.96 * std_err),
    };

    let target_secs = (((bd.combat_secs / 10.0).round() as u32) * 10).clamp(60, 600);
    let iters = seeds.count.max(50);

    let mut simc = Command::new(simc_bin);
    simc.arg(simc_profile);
    if let Some(t) = &talents.simc_override {
        simc.arg(format!("talents={t}"));
    }
    let output = simc
        .arg("deterministic=1")
        .arg("threads=8")
        .arg("vary_combat_length=0.2")
        .arg("fixed_time=1")
        .arg(format!("max_time={target_secs}"))
        .arg(format!("iterations={iters}"))
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
        talents_override: talents.simc_override,
        unmapped_talents: talents.unmapped,
        unmodelled_gear: eval_bundle
            .unmodelled_gear
            .iter()
            .map(|u| format!("{:?}: {} ({})", u.slot, u.name, u.spell.0))
            .collect(),
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
                    if raw_name == "mental_fortitude" {
                        continue;
                    }
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
        "ignite_periodic" => "ignite".into(),
        "meteor_impact" => "meteor".into(),
        "meteor_burn_periodic" => "meteor_burn".into(),
        "meteorite_impact" | "meteorite_bug_impact" => "meteorite".into(),
        "conflagration_periodic" => "conflagration".into(),
        "rune_of_unleashed_fire_lingering" => "rune_of_unleashed_fire".into(),
        "voracious_heart_of_ulatek" => "devour_morsel".into(),
        "zuljins_guillotine_technique" | "perfected_guillotine" => "guillotine".into(),
        "shadow_blades_attack" | "shadow_blades_hit" => "shadow_blades_hit".into(),
        "eviscerate_bonus" | "eviscerate_shadow" => "eviscerate_shadow".into(),
        "black_powder_bonus" | "black_powder_shadow" => "black_powder_shadow".into(),
        "secret_technique_clones" | "secret_technique_clone" => "secret_technique_clone".into(),
        "secret_technique_player" | "secret_technique" => "secret_technique".into(),
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
        "ignite" => "Ignite".into(),
        "meteor" => "Meteor (Impact)".into(),
        "meteor_burn" => "Meteor Burn".into(),
        "meteorite" => "Meteorite".into(),
        "conflagration" => "Conflagration".into(),
        "devour_morsel" => "Devour Morsel (Trinket)".into(),
        "guillotine" => "Guillotine (Trinket)".into(),
        "vile_vial_of_volatile_venom" => "Vile Vial of Volatile Venom (Trinket)".into(),
        "gebbos_bottomless_bag" => "Gebbo's Bottomless Bag (Trinket)".into(),
        "rune_of_unleashed_fire" => "Rune of Unleashed Fire (Enchant)".into(),
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
        let is_secondary_subhit = r.name.ends_with("(OH)")
            || r.name.ends_with("(2)")
            || r.name.ends_with("(3)")
            || r.name.ends_with("(4)")
            || (r.dps == 0.0
                && (key == "execute"
                    || key == "raging_blow"
                    || key == "rampage"
                    || key == "execution_sentence"));
        if !is_secondary_subhit {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tolerance_is_half_percent() {
        assert!((DEFAULT_PARITY_TOLERANCE - 0.005).abs() < 1e-12);
        let make = |ratio: f64| SimcParityReport {
            run_name: "test".into(),
            spec_id: SpecId(71),
            spec_name: "Arms".into(),
            hero_tree_name: Some("Slayer".into()),
            simc_profile: PathBuf::new(),
            dump_diffs_applied: 0,
            talents_override: None,
            unmapped_talents: Vec::new(),
            unmodelled_gear: Vec::new(),
            portunus_dps: Estimate {
                n: 30,
                mean: 60_000.0 * ratio,
                std_err: 100.0,
                ci95: (59_800.0, 60_200.0),
            },
            portunus_combat_secs: 150.0,
            simc_dps: 60_000.0,
            dps_ratio: ratio,
            rows: Vec::new(),
        };
        assert!(make(1.001).within_tolerance(DEFAULT_PARITY_TOLERANCE));
        assert!(make(0.998).within_tolerance(DEFAULT_PARITY_TOLERANCE));
        assert!(make(1.005).within_tolerance(DEFAULT_PARITY_TOLERANCE));
        assert!(!make(1.006).within_tolerance(DEFAULT_PARITY_TOLERANCE));
        assert!(!make(0.994).within_tolerance(DEFAULT_PARITY_TOLERANCE));
    }
}
