//! Parser and structure-preserving [`GameData`] syncer for SimulationCraft's
//! `SpellDataDump` files (`target/simc/SpellDataDump/*.txt`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use portunus_core::{AuraId, SimDuration, SpecId, SpellId};
use portunus_gamedata::aura::{AuraDef, RefreshRule};
use portunus_gamedata::effect::{Coefficient, Effect, ModKind, ModScope, Modifier};
use portunus_gamedata::{GameBuild, GameData};
use serde::{Deserialize, Serialize};

use crate::{GameDataSource, IngestError};

const CLASS_DUMP_FILES: &[&str] = &[
    "warrior.txt",
    "shaman.txt",
    "paladin.txt",
    "deathknight.txt",
    "hunter.txt",
    "rogue.txt",
    "mage.txt",
    "warlock.txt",
    "priest.txt",
    "monk.txt",
    "druid.txt",
    "demonhunter.txt",
    "evoker.txt",
];

/// One effect slot (`#1`, `#2`, ...) parsed from a `SpellDataDump` entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DumpEffect {
    pub index: u8,
    pub effect_id: u32,
    pub kind: String,
    pub base_value: Option<f64>,
    pub scaled_value: Option<f64>,
    pub sp_coefficient: Option<f64>,
    pub ap_coefficient: Option<f64>,
    pub weapon_pct: Option<f64>,
    pub period_ms: Option<u32>,
    pub radius_yards: Option<f64>,
    pub chain_multiplier: Option<f64>,
    pub trigger_spell: Option<SpellId>,
    pub resource_name: Option<String>,
    pub affected_spells: Vec<SpellId>,
}

/// One spell record (`Name : ... (id=N)`) parsed from `SpellDataDump`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DumpSpell {
    pub id: SpellId,
    pub name: String,
    pub is_passive: bool,
    pub is_hidden: bool,
    pub is_channeled: bool,
    pub class: Option<String>,
    pub school: Option<String>,
    pub cast_time_ms: Option<u32>,
    pub gcd_ms: Option<u32>,
    pub duration_ms: Option<u32>,
    pub cooldown_ms: Option<u32>,
    pub charges: Option<(u8, u32)>,
    pub range_yards: Option<f64>,
    pub velocity: Option<f64>,
    pub max_stacks: Option<u8>,
    pub rppm: Option<(f64, bool)>,
    pub internal_cooldown_ms: Option<u32>,
    pub proc_chance: Option<f64>,
    pub resources: Vec<(String, f64)>,
    pub triggered_by: Vec<SpellId>,
    pub description_refs: Vec<SpellId>,
    pub spelldesc_parent: Option<SpellId>,
    pub effects: Vec<DumpEffect>,
}

/// Complete catalog of parsed spells from SimulationCraft's `SpellDataDump`.
#[derive(Debug, Clone)]
pub struct SpellDump {
    pub build: GameBuild,
    pub spells: BTreeMap<SpellId, DumpSpell>,
    /// Reverse index: parent spell ID -> child spells that list `Triggered By: ... (parent)`
    /// or `Description: $@spelldesc<parent>`.
    pub children_of: BTreeMap<SpellId, Vec<SpellId>>,
}

/// A single difference found (or synced) between [`GameData`] and [`SpellDump`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DumpDiff {
    pub owner_kind: &'static str,
    pub id: u32,
    pub name: String,
    pub field: String,
    pub current: f64,
    pub dump: f64,
    pub dump_spell_id: SpellId,
}

impl SpellDump {
    /// Loads `build_info.txt` and all 13 class dump files from `dir`
    /// (e.g. `target/simc/SpellDataDump`).
    pub fn from_dir(dir: &Path) -> Result<Self, IngestError> {
        let build_path = dir.join("build_info.txt");
        let build = if build_path.exists() {
            let text = fs::read_to_string(&build_path)
                .map_err(|e| IngestError::Io(format!("{}: {e}", build_path.display())))?;
            parse_build_info(&text)
        } else {
            GameBuild {
                version: "12.1.0".into(),
                build: 69933,
            }
        };

        let mut dump = Self {
            build,
            spells: BTreeMap::new(),
            children_of: BTreeMap::new(),
        };

        let mut loaded_any = false;
        for &filename in CLASS_DUMP_FILES {
            let path = dir.join(filename);
            if !path.exists() {
                continue;
            }
            let content = fs::read_to_string(&path)
                .map_err(|e| IngestError::Io(format!("{}: {e}", path.display())))?;
            dump.parse_into(&content);
            loaded_any = true;
        }

        if !loaded_any {
            let all_path = dir.join("allspells.txt");
            let content = fs::read_to_string(&all_path)
                .map_err(|e| IngestError::Io(format!("{}: {e}", all_path.display())))?;
            dump.parse_into(&content);
        }

        dump.rebuild_index();
        Ok(dump)
    }

    /// Parses a single `SpellDataDump` text slice into a [`SpellDump`].
    pub fn parse_str(text: &str) -> Self {
        let mut dump = Self {
            build: parse_build_info(text),
            spells: BTreeMap::new(),
            children_of: BTreeMap::new(),
        };
        dump.parse_into(text);
        dump.rebuild_index();
        dump
    }

    fn parse_into(&mut self, text: &str) {
        let mut current_block: Vec<&str> = Vec::new();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            if line.starts_with("Name             : ") && !current_block.is_empty() {
                if let Some(spell) = parse_spell_block(&current_block) {
                    self.spells.insert(spell.id, spell);
                }
                current_block.clear();
            }
            if !current_block.is_empty() || line.starts_with("Name             : ") {
                current_block.push(line);
            }
        }
        if !current_block.is_empty() {
            if let Some(spell) = parse_spell_block(&current_block) {
                self.spells.insert(spell.id, spell);
            }
        }
    }

    fn rebuild_index(&mut self) {
        self.children_of.clear();
        for spell in self.spells.values() {
            for &parent in &spell.triggered_by {
                let list = self.children_of.entry(parent).or_default();
                if !list.contains(&spell.id) {
                    list.push(spell.id);
                }
            }
            if let Some(parent) = spell.spelldesc_parent {
                let list = self.children_of.entry(parent).or_default();
                if !list.contains(&spell.id) {
                    list.push(spell.id);
                }
            }
        }
    }

    /// Resolves direct-damage coefficients `(sp_coeff, ap_coeff, weapon_pct, source_spell)`
    /// for `spell_id`. If `spell_id` is a cast/missile wrapper with no direct
    /// damage on its own record, follows `trigger_spell`, `$<id>s` description
    /// references, and `Triggered By` child spells.
    pub fn resolve_direct_damage(&self, spell_id: SpellId) -> Vec<(Coefficient, SpellId)> {
        let Some(root) = self.spells.get(&spell_id) else {
            return Vec::new();
        };

        let direct_from = |sp: &DumpSpell| -> Vec<(Coefficient, SpellId)> {
            let mut out = Vec::new();
            let mut w_pct: Option<f64> = None;
            for eff in &sp.effects {
                if eff.kind.contains("Weapon Damage") {
                    if let Some(wp) = eff.weapon_pct.or(eff.scaled_value).or(eff.base_value) {
                        if wp > 0.0 {
                            w_pct = Some(wp / 100.0);
                        }
                    }
                }
            }
            for eff in &sp.effects {
                let is_direct_dmg = eff.kind.contains("School Damage")
                    || eff.kind.contains("Normalized Weapon Damage")
                    || eff.kind.contains("Weapon Damage");
                if !is_direct_dmg {
                    continue;
                }
                if let Some(spc) = eff.sp_coefficient {
                    if spc > 0.0 {
                        out.push((Coefficient::SpellPower(spc), sp.id));
                        continue;
                    }
                }
                if let Some(apc) = eff.ap_coefficient {
                    if apc > 0.0 {
                        out.push((Coefficient::AttackPower(apc), sp.id));
                        continue;
                    }
                }
                if let Some(wp) = eff.weapon_pct.or(w_pct) {
                    if wp > 0.0 && eff.kind.contains("Normalized Weapon") {
                        out.push((Coefficient::WeaponDamage(wp), sp.id));
                    }
                }
            }
            out
        };

        let own = direct_from(root);
        if !own.is_empty() {
            return own;
        }

        // Check direct trigger_spell effects on root.
        let mut candidates: Vec<SpellId> = Vec::new();
        for eff in &root.effects {
            if let Some(ts) = eff.trigger_spell {
                if !candidates.contains(&ts) {
                    candidates.push(ts);
                }
            }
        }
        // Check $<id>s description references on root.
        for &dref in &root.description_refs {
            if !candidates.contains(&dref) {
                candidates.push(dref);
            }
        }
        // Check children that declare Triggered By: root or $@spelldesc<root>.
        if let Some(children) = self.children_of.get(&spell_id) {
            for &cid in children {
                if !candidates.contains(&cid) {
                    candidates.push(cid);
                }
            }
        }

        let mut resolved = Vec::new();
        for cid in candidates {
            if let Some(child) = self.spells.get(&cid) {
                let sub = direct_from(child);
                if !sub.is_empty() {
                    resolved.extend(sub);
                }
            }
        }
        resolved
    }

    /// Resolves periodic-damage `(Coefficient, Option<period_ms>, source_spell)`
    /// for an aura `aura_id`.
    pub fn resolve_periodic_damage(
        &self,
        aura_id: AuraId,
    ) -> Option<(Coefficient, Option<u32>, SpellId)> {
        let sid = SpellId(aura_id.0);
        let root = self.spells.get(&sid)?;

        let periodic_from = |sp: &DumpSpell| -> Option<(Coefficient, Option<u32>, SpellId)> {
            for eff in &sp.effects {
                if !eff.kind.contains("Periodic Damage") {
                    continue;
                }
                if let Some(spc) = eff.sp_coefficient {
                    if spc > 0.0 {
                        return Some((Coefficient::SpellPower(spc), eff.period_ms, sp.id));
                    }
                }
                if let Some(apc) = eff.ap_coefficient {
                    if apc > 0.0 {
                        return Some((Coefficient::AttackPower(apc), eff.period_ms, sp.id));
                    }
                }
            }
            None
        };

        if let Some(found) = periodic_from(root) {
            return Some(found);
        }

        for eff in &root.effects {
            if let Some(ts) = eff.trigger_spell {
                if let Some(child) = self.spells.get(&ts) {
                    if let Some(found) = periodic_from(child) {
                        return Some(found);
                    }
                }
            }
        }
        if let Some(children) = self.children_of.get(&sid) {
            for &cid in children {
                if let Some(child) = self.spells.get(&cid) {
                    if let Some(found) = periodic_from(child) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
}

/// Audits `data` against `dump`, returning all direct and periodic coefficient
/// differences without mutating `data`.
pub fn audit_game_data(data: &GameData, dump: &SpellDump) -> Vec<DumpDiff> {
    let mut clone = data.clone();
    sync_game_data_with_dump(&mut clone, dump, &BTreeSet::new())
}

/// Maps a [`SpecId`] to its 12.1 spec-wide passive tuning aura in `SpellDataDump`.
pub fn spec_passive_aura_id(spec: SpecId) -> Option<AuraId> {
    let id = match spec.0 {
        62 => 137021,    // Arcane Mage
        63 => 137019,    // Fire Mage
        64 => 137020,    // Frost Mage
        70 => 137027,    // Retribution Paladin
        71 => 137049,    // Arms Warrior
        72 => 137050,    // Fury Warrior
        102 => 137013,   // Balance Druid
        103 => 137011,   // Feral Druid
        251 => 137006,   // Frost Death Knight
        252 => 137007,   // Unholy Death Knight
        253 => 137015,   // Beast Mastery Hunter
        254 => 137016,   // Marksmanship Hunter
        255 => 137017,   // Survival Hunter
        258 => 137033,   // Shadow Priest
        259 => 137037,   // Assassination Rogue
        260 => 137036,   // Outlaw Rogue
        261 => 137035,   // Subtlety Rogue
        262 => 137040,   // Elemental Shaman
        263 => 137041,   // Enhancement Shaman
        265 => 137043,   // Affliction Warlock
        266 => 137044,   // Demonology Warlock
        267 => 137046,   // Destruction Warlock
        269 => 137025,   // Windwalker Monk
        577 => 212612,   // Havoc Demon Hunter
        1480 => 1213636, // Devourer Demon Hunter
        1467 => 356809,  // Devastation Evoker
        1473 => 396186,  // Augmentation Evoker
        _ => return None,
    };
    Some(AuraId(id))
}

/// Extracts `DamageDonePct`, `PetDamagePct`, `GuardianDamagePct`, and per-spell
/// `DamageDonePct` modifiers from a spec passive entry in `dump`.
pub fn extract_spec_passive_aura(
    aura_id: AuraId,
    dump: &SpellDump,
    known_spells: &BTreeMap<SpellId, portunus_gamedata::SpellDef>,
) -> Option<AuraDef> {
    let sp = dump.spells.get(&SpellId(aura_id.0))?;
    let mut modifiers: Vec<Modifier> = Vec::new();
    let mut direct_spell_mods: BTreeSet<(SpellId, i64)> = BTreeSet::new();

    for eff in &sp.effects {
        let val = eff.scaled_value.or(eff.base_value).unwrap_or(0.0);
        if val.abs() < 1e-6 {
            continue;
        }
        if eff.kind.contains("Spell Direct Amount (0)") && eff.index == 1 {
            modifiers.push(Modifier {
                scope: ModScope::Abilities,
                kind: ModKind::DamageDonePct,
                value: val,
                per_stack: false,
                condition: None,
            });
        } else if eff.kind.contains("Modify Auto Attack Damage Done%") {
            modifiers.push(Modifier {
                scope: ModScope::AutoAttack,
                kind: ModKind::DamageDonePct,
                value: val,
                per_stack: false,
                condition: None,
            });
        } else if eff.kind.contains("Modify Pet Damage Done%") {
            modifiers.push(Modifier {
                scope: ModScope::All,
                kind: ModKind::PetDamagePct,
                value: val,
                per_stack: false,
                condition: None,
            });
        } else if eff.kind.contains("Modify Guardian Damage Done%") {
            modifiers.push(Modifier {
                scope: ModScope::All,
                kind: ModKind::GuardianDamagePct,
                value: val,
                per_stack: false,
                condition: None,
            });
        } else if (eff.kind.contains("Spell Direct Amount (0)")
            || eff.kind.contains("Spell Periodic Amount (22)"))
            && eff.index > 2
            && !eff.affected_spells.is_empty()
            && eff.affected_spells.len() <= 55
        {
            let is_periodic = eff.kind.contains("Spell Periodic Amount (22)");
            let val_key = (val * 100.0).round() as i64;
            let mut seen_in_effect: BTreeSet<SpellId> = BTreeSet::new();
            for &target_spell in &eff.affected_spells {
                let mut candidates = vec![target_spell];
                if target_spell == SpellId(285466) {
                    candidates.push(SpellId(77451));
                } else if target_spell == SpellId(117014) {
                    candidates.push(SpellId(462840));
                }
                for cand in candidates {
                    if !known_spells.contains_key(&cand) || !seen_in_effect.insert(cand) {
                        continue;
                    }
                    if is_periodic && direct_spell_mods.contains(&(cand, val_key)) {
                        continue;
                    }
                    if !is_periodic {
                        direct_spell_mods.insert((cand, val_key));
                    }
                    modifiers.push(Modifier {
                        scope: ModScope::Spell(cand),
                        kind: ModKind::DamageDonePct,
                        value: val,
                        per_stack: false,
                        condition: None,
                    });
                }
            }
        }
    }

    Some(AuraDef {
        id: aura_id,
        name: sp.name.clone(),
        duration: None,
        max_stacks: 1,
        refresh: RefreshRule::Replace,
        periodic: None,
        value: None,
        modifiers,
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
    })
}

/// Synchronizes [`GameData`] coefficients with `dump` for all spells and auras
/// not belonging to `skip_specs`, returning every applied [`DumpDiff`].
pub fn sync_game_data_with_dump(
    data: &mut GameData,
    dump: &SpellDump,
    skip_specs: &BTreeSet<SpecId>,
) -> Vec<DumpDiff> {
    data.build = dump.build.clone();

    let mut skipped_spells: BTreeSet<SpellId> = BTreeSet::new();
    let mut skipped_auras: BTreeSet<AuraId> = BTreeSet::new();
    for (&spec_id, spec_def) in &data.specs {
        if skip_specs.contains(&spec_id) {
            skipped_spells.extend(&spec_def.baseline_spells);
            skipped_auras.extend(&spec_def.baseline_auras);
        }
    }

    let mut diffs = Vec::new();

    // Sync missing spec-wide passive tuning auras (1370xx / 212612 / 1213636 / 356809 / 396186).
    let spec_ids: Vec<SpecId> = data.specs.keys().copied().collect();
    for spec_id in spec_ids {
        if skip_specs.contains(&spec_id) {
            continue;
        }
        let Some(passive_id) = spec_passive_aura_id(spec_id) else {
            continue;
        };
        // Only add per-spell modifiers from dump if the existing passive aura
        // doesn't already have hand-authored per-spell modifiers.
        if let Some(aura_def) = extract_spec_passive_aura(passive_id, dump, &data.spells) {
            if let Some(existing) = data.auras.get_mut(&passive_id) {
                let has_hand_spell_mods = existing.modifiers.iter().any(|em| {
                    matches!(em.scope, ModScope::Spell(_)) && em.kind == ModKind::DamageDonePct
                });
                for m in aura_def.modifiers {
                    if has_hand_spell_mods && matches!(m.scope, ModScope::Spell(_)) {
                        continue;
                    }
                    let already_has = existing
                        .modifiers
                        .iter()
                        .any(|em| em.scope == m.scope && em.kind == m.kind);
                    if !already_has {
                        diffs.push(DumpDiff {
                            owner_kind: "spec_aura",
                            id: passive_id.0,
                            name: existing.name.clone(),
                            field: format!("modifier({:?})", m.scope),
                            current: 0.0,
                            dump: m.value,
                            dump_spell_id: SpellId(passive_id.0),
                        });
                        existing.modifiers.push(m);
                    }
                }
            } else {
                let first_val = aura_def.modifiers.first().map_or(0.0, |m| m.value);
                diffs.push(DumpDiff {
                    owner_kind: "spec_aura",
                    id: passive_id.0,
                    name: aura_def.name.clone(),
                    field: "damage_done_pct".into(),
                    current: 0.0,
                    dump: first_val,
                    dump_spell_id: SpellId(passive_id.0),
                });
                data.auras.insert(passive_id, aura_def);
            }
        }
        if spec_id == SpecId(262) {
            if let Some(m_aura) = data.auras.get_mut(&AuraId(168534)) {
                if !m_aura
                    .modifiers
                    .iter()
                    .any(|m| m.kind == ModKind::MasteryDamagePct)
                {
                    m_aura.modifiers.push(Modifier {
                        scope: ModScope::Abilities,
                        kind: ModKind::MasteryDamagePct,
                        value: 0.365,
                        per_stack: false,
                        condition: None,
                    });
                }
            }
            if let Some(s_def) = data.specs.get_mut(&SpecId(262)) {
                s_def.base_mastery_pct = 15.0;
                s_def.mastery_coef = 1.875;
            }
            if let Some(vb) = data.spells.get_mut(&SpellId(470057)) {
                if let Some(cd) = &mut vb.cooldown {
                    cd.duration = SimDuration(15_000);
                }
            }
            if let Some(asc) = data.spells.get_mut(&SpellId(114050)) {
                if let Some(cd) = &mut asc.cooldown {
                    cd.duration = SimDuration(120_000);
                }
            }
        }
        if data.auras.contains_key(&passive_id) {
            if let Some(spec_def) = data.specs.get_mut(&spec_id) {
                if !spec_def.baseline_auras.contains(&passive_id) {
                    spec_def.baseline_auras.insert(0, passive_id);
                }
            }
        }
    }

    for (&spell_id, spell_def) in &mut data.spells {
        if skipped_spells.contains(&spell_id) || spell_id.0 >= 900_000 {
            continue;
        }
        let resolved = if spell_id == SpellId(462840) {
            dump.resolve_direct_damage(SpellId(117014))
                .into_iter()
                .map(|(c, sid)| match c {
                    Coefficient::SpellPower(v) => (Coefficient::SpellPower(v * 0.40), sid),
                    other => (other, sid),
                })
                .collect()
        } else {
            dump.resolve_direct_damage(spell_id)
        };
        if resolved.is_empty() {
            continue;
        }
        let overload_scale = if spell_id == SpellId(381725) || spell_id == SpellId(120588) {
            0.125
        } else if spell_def.name.contains("Overload") {
            0.25
        } else {
            1.0
        };
        let scaled_resolved: Vec<(Coefficient, SpellId)> = resolved
            .into_iter()
            .map(|(c, sid)| {
                let c2 = match c {
                    Coefficient::SpellPower(v) => Coefficient::SpellPower(v * overload_scale),
                    Coefficient::AttackPower(v) => Coefficient::AttackPower(v * overload_scale),
                    other => other,
                };
                (c2, sid)
            })
            .collect();
        let mut cursor = 0usize;
        sync_effects_slice(
            &mut spell_def.effects,
            spell_id.0,
            &spell_def.name,
            &scaled_resolved,
            &mut cursor,
            &mut diffs,
        );
    }

    for (&aura_id, aura_def) in &mut data.auras {
        if skipped_auras.contains(&aura_id) || aura_id.0 >= 900_000 {
            continue;
        }
        let Some((dump_coeff, dump_period, src_spell)) = dump.resolve_periodic_damage(aura_id)
        else {
            continue;
        };
        if let Some(per) = &mut aura_def.periodic {
            let mut cursor = 0usize;
            sync_effects_slice(
                &mut per.effects,
                aura_id.0,
                &aura_def.name,
                &[(dump_coeff, src_spell)],
                &mut cursor,
                &mut diffs,
            );
            if let Some(p_ms) = dump_period {
                if p_ms > 0 && per.period.0 != p_ms {
                    diffs.push(DumpDiff {
                        owner_kind: "aura",
                        id: aura_id.0,
                        name: aura_def.name.clone(),
                        field: "period_ms".into(),
                        current: f64::from(per.period.0),
                        dump: f64::from(p_ms),
                        dump_spell_id: src_spell,
                    });
                    per.period = SimDuration(p_ms);
                }
            }
        }
    }

    diffs
}

fn match_coeff_kind(current: &Coefficient, dump: Coefficient) -> Option<(f64, f64)> {
    match (*current, dump) {
        (Coefficient::SpellPower(c), Coefficient::SpellPower(d)) => Some((c, d)),
        (Coefficient::AttackPower(c), Coefficient::AttackPower(d)) => Some((c, d)),
        (Coefficient::WeaponDamage(c), Coefficient::WeaponDamage(d)) => Some((c, d)),
        _ => None,
    }
}

fn sync_effects_slice(
    effects: &mut [Effect],
    spell_id: u32,
    spell_name: &str,
    resolved: &[(Coefficient, SpellId)],
    cursor: &mut usize,
    diffs: &mut Vec<DumpDiff>,
) {
    for eff in effects {
        match eff {
            Effect::Damage {
                amount, unmodified, ..
            } if !*unmodified => {
                if let Some(&(dump_coeff, src_spell)) =
                    resolved.get(*cursor).or_else(|| resolved.first())
                {
                    if let Some((cur_v, new_v)) = match_coeff_kind(amount, dump_coeff) {
                        let ratio = if new_v.abs() > 1e-9 {
                            cur_v / new_v
                        } else {
                            0.0
                        };
                        let intentional_multiple = (ratio - 2.0).abs() < 1e-4
                            || (ratio - 0.5).abs() < 1e-4
                            || (ratio - 0.4).abs() < 1e-4;
                        if (cur_v - new_v).abs() > 1e-4 && !intentional_multiple {
                            diffs.push(DumpDiff {
                                owner_kind: "spell",
                                id: spell_id,
                                name: spell_name.to_owned(),
                                field: format!("damage_coeff[{cursor}]"),
                                current: cur_v,
                                dump: new_v,
                                dump_spell_id: src_spell,
                            });
                            *amount = dump_coeff;
                        }
                        *cursor += 1;
                    }
                }
            }
            Effect::ConsumeStacks { then, .. } => {
                sync_effects_slice(then, spell_id, spell_name, resolved, cursor, diffs);
            }
            _ => {}
        }
    }
}

/// [`GameDataSource`] that loads a base source (e.g. [`crate::RonFile`]) and
/// synchronizes its spell and aura coefficients against a `SpellDataDump`
/// directory.
pub struct DumpSyncedSource<S> {
    pub base: S,
    pub dump_dir: PathBuf,
    pub skip_specs: BTreeSet<SpecId>,
}

impl<S: GameDataSource> GameDataSource for DumpSyncedSource<S> {
    fn load(&self) -> Result<GameData, IngestError> {
        let mut data = self.base.load()?;
        let dump = SpellDump::from_dir(&self.dump_dir)?;
        let _ = sync_game_data_with_dump(&mut data, &dump, &self.skip_specs);
        Ok(data)
    }
}

fn parse_build_info(text: &str) -> GameBuild {
    // e.g. "SimulationCraft 1210-01 for World of Warcraft 12.1.0.69933 Live"
    let first_line = text.lines().next().unwrap_or("");
    if let Some(pos) = first_line.find("World of Warcraft ") {
        let after = &first_line[pos + "World of Warcraft ".len()..];
        let token = after.split_whitespace().next().unwrap_or("12.1.0.69933");
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() >= 4 {
            let version = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
            let build = parts[3].parse::<u32>().unwrap_or(69933);
            return GameBuild { version, build };
        }
    }
    GameBuild {
        version: "12.1.0".into(),
        build: 69933,
    }
}

fn parse_spell_block(lines: &[&str]) -> Option<DumpSpell> {
    let header = *lines.first()?;
    let rest = header.strip_prefix("Name             : ")?;
    let id_pos = rest.find("(id=")?;
    let name = rest[..id_pos].trim().to_owned();
    let after_id = &rest[id_pos + 4..];
    let end_paren = after_id.find(')')?;
    let id_num = after_id[..end_paren].trim().parse::<u32>().ok()?;
    let flags_part = &after_id[end_paren + 1..];
    let is_passive = flags_part.contains("Passive");
    let is_hidden = flags_part.contains("Hidden");

    let mut spell = DumpSpell {
        id: SpellId(id_num),
        name,
        is_passive,
        is_hidden,
        is_channeled: false,
        class: None,
        school: None,
        cast_time_ms: None,
        gcd_ms: None,
        duration_ms: None,
        cooldown_ms: None,
        charges: None,
        range_yards: None,
        velocity: None,
        max_stacks: None,
        rppm: None,
        internal_cooldown_ms: None,
        proc_chance: None,
        resources: Vec::new(),
        triggered_by: Vec::new(),
        description_refs: Vec::new(),
        spelldesc_parent: None,
        effects: Vec::new(),
    };

    let mut idx = 1usize;
    while idx < lines.len() {
        let line = lines[idx];
        if let Some(val) = line.strip_prefix("Class            : ") {
            spell.class = Some(val.trim().to_owned());
        } else if let Some(val) = line.strip_prefix("School           : ") {
            spell.school = Some(val.trim().to_owned());
        } else if let Some(val) = line.strip_prefix("Cast Time        : ") {
            spell.cast_time_ms = parse_duration_ms(val);
        } else if let Some(val) = line.strip_prefix("GCD              : ") {
            spell.gcd_ms = parse_duration_ms(val);
        } else if let Some(val) = line.strip_prefix("Duration         : ") {
            spell.duration_ms = parse_duration_ms(val);
        } else if let Some(val) = line.strip_prefix("Cooldown         : ") {
            spell.cooldown_ms = parse_duration_ms(val);
        } else if let Some(val) = line.strip_prefix("Internal Cooldown: ") {
            spell.internal_cooldown_ms = parse_duration_ms(val);
        } else if let Some(val) = line.strip_prefix("Charges          : ") {
            spell.charges = parse_charges(val);
        } else if let Some(val) = line.strip_prefix("Range            : ") {
            spell.range_yards = parse_leading_f64(val.split('-').next_back().unwrap_or(val));
        } else if let Some(val) = line.strip_prefix("Velocity         : ") {
            spell.velocity = parse_leading_f64(val);
        } else if let Some(val) = line.strip_prefix("Stacks           : ") {
            if let Some(n) = parse_leading_f64(val) {
                spell.max_stacks = u8::try_from(n.round() as u32).ok();
            }
        } else if let Some(val) = line.strip_prefix("Real PPM         : ") {
            if let Some(r) = parse_leading_f64(val) {
                let hasted = val.contains("Haste");
                spell.rppm = Some((r, hasted));
            }
        } else if let Some(val) = line.strip_prefix("Proc Chance      : ") {
            if let Some(p) = parse_leading_f64(val) {
                spell.proc_chance = Some(p);
            }
        } else if let Some(val) = line.strip_prefix("Resource         : ") {
            if let Some(res) = parse_resource_line(val) {
                spell.resources.push(res);
            }
        } else if let Some(val) = line.strip_prefix("Triggered By     : ") {
            spell.triggered_by.extend(extract_paren_spell_ids(val));
        } else if let Some(val) = line.strip_prefix("Attributes       : ") {
            if val.contains("Is Channelled") || val.contains("Is Self Channelled") {
                spell.is_channeled = true;
            }
            if val.contains("Passive (6)") {
                spell.is_passive = true;
            }
        } else if let Some(val) = line.strip_prefix("Description      : ") {
            let trimmed = val.trim();
            if let Some(after) = trimmed.strip_prefix("$@spelldesc") {
                if let Ok(pid) = after.trim().parse::<u32>() {
                    spell.spelldesc_parent = Some(SpellId(pid));
                }
            }
            extract_dollar_spell_refs(val, &mut spell.description_refs);
        } else if line.starts_with('#') && line.contains("(id=") && line.contains(" : ") {
            if let Some((eff, next_idx)) = parse_effect(lines, idx) {
                spell.effects.push(eff);
                idx = next_idx;
                continue;
            }
        } else if line.contains('$') {
            extract_dollar_spell_refs(line, &mut spell.description_refs);
        }
        idx += 1;
    }

    Some(spell)
}

fn parse_effect(lines: &[&str], start: usize) -> Option<(DumpEffect, usize)> {
    let first = lines[start];
    let after_hash = first.strip_prefix('#')?;
    let space_pos = after_hash.find(' ')?;
    let index = after_hash[..space_pos].parse::<u8>().ok()?;
    let id_start = first.find("(id=")? + 4;
    let id_end = first[id_start..].find(')')? + id_start;
    let effect_id = first[id_start..id_end].parse::<u32>().ok()?;
    let colon_pos = first.find(" : ")?;
    let kind = first[colon_pos + 3..].trim().to_owned();

    let period_ms = if let Some(every_pos) = kind.find("every ") {
        parse_duration_ms(&kind[every_pos + 6..])
    } else {
        None
    };

    let mut eff = DumpEffect {
        index,
        effect_id,
        kind,
        base_value: None,
        scaled_value: None,
        sp_coefficient: None,
        ap_coefficient: None,
        weapon_pct: None,
        period_ms,
        radius_yards: None,
        chain_multiplier: None,
        trigger_spell: None,
        resource_name: None,
        affected_spells: Vec::new(),
    };

    let mut idx = start + 1;
    while idx < lines.len() {
        let line = lines[idx];
        if !line.starts_with("                   ") {
            break;
        }
        let detail = line.trim();
        if detail.starts_with("Affected Spells") {
            eff.affected_spells.extend(extract_paren_spell_ids(detail));
        } else {
            for seg in detail.split('|') {
                let seg = seg.trim();
                if let Some(v) = seg.strip_prefix("Base Value:") {
                    eff.base_value = parse_leading_f64(v);
                } else if let Some(v) = seg.strip_prefix("Scaled Value:") {
                    eff.scaled_value = parse_leading_f64(v);
                } else if let Some(v) = seg.strip_prefix("SP Coefficient:") {
                    eff.sp_coefficient = parse_leading_f64(v);
                } else if let Some(v) = seg.strip_prefix("AP Coefficient:") {
                    eff.ap_coefficient = parse_leading_f64(v);
                } else if let Some(v) = seg.strip_prefix("Weapon Damage%:") {
                    eff.weapon_pct = parse_leading_f64(v);
                } else if let Some(v) = seg.strip_prefix("Radius:") {
                    eff.radius_yards = parse_leading_f64(v.split('-').next_back().unwrap_or(v));
                } else if let Some(v) = seg.strip_prefix("Chain Multiplier:") {
                    eff.chain_multiplier = parse_leading_f64(v);
                } else if let Some(v) = seg.strip_prefix("Trigger Spell:") {
                    if let Some(tid) = parse_leading_f64(v) {
                        eff.trigger_spell = Some(SpellId(tid.round() as u32));
                    }
                } else if let Some(v) = seg.strip_prefix("Resource:") {
                    eff.resource_name = Some(v.trim().to_owned());
                }
            }
        }
        idx += 1;
    }

    Some((eff, idx))
}

fn parse_duration_ms(s: &str) -> Option<u32> {
    let s = s.trim();
    let num = parse_leading_f64(s)?;
    if num < 0.0 {
        return None;
    }
    if s.contains("milliseconds") || s.contains("_ms") {
        Some(num.round() as u32)
    } else if s.contains("minutes") || s.contains("minute") {
        Some((num * 60_000.0).round() as u32)
    } else if s.contains("hours") || s.contains("hour") {
        Some((num * 3_600_000.0).round() as u32)
    } else {
        Some((num * 1000.0).round() as u32)
    }
}

fn parse_charges(s: &str) -> Option<(u8, u32)> {
    // e.g. "1 (8 seconds cooldown)" or "2 (12 seconds cooldown)"
    let charges = u8::try_from(parse_leading_f64(s)?.round() as u32).ok()?;
    let open = s.find('(')?;
    let close = s[open..].find(')')? + open;
    let cd_ms = parse_duration_ms(&s[open + 1..close])?;
    Some((charges, cd_ms))
}

fn parse_resource_line(s: &str) -> Option<(String, f64)> {
    // e.g. "30 Rage (1) (id=9599)" or "60 Maelstrom (11) (id=215295)"
    let s = s.trim();
    let amount = parse_leading_f64(s)?;
    let rest = s
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '-' || c == '%')
        .trim();
    let paren = rest.find('(').unwrap_or(rest.len());
    let name = rest[..paren].trim().to_lowercase();
    Some((name, amount))
}

fn parse_leading_f64(s: &str) -> Option<f64> {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    s[..end].parse::<f64>().ok()
}

fn extract_paren_spell_ids(s: &str) -> Vec<SpellId> {
    let mut out = Vec::new();
    let mut rem = s;
    while let Some(open) = rem.find('(') {
        let after = &rem[open + 1..];
        let Some(close) = after.find(')') else {
            break;
        };
        let inside = after[..close].trim();
        let token = inside.split_whitespace().next().unwrap_or("");
        if let Ok(id) = token.parse::<u32>() {
            if id > 0 {
                out.push(SpellId(id));
            }
        }
        rem = &after[close + 1..];
    }
    out
}

fn extract_dollar_spell_refs(s: &str, out: &mut Vec<SpellId>) {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end > start && end < bytes.len() && (bytes[end] == b's' || bytes[end] == b'o') {
                if let Ok(id) = s[start..end].parse::<u32>() {
                    if id >= 100 && !out.contains(&SpellId(id)) {
                        out.push(SpellId(id));
                    }
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DUMP: &str = "\
SimulationCraft 1210-01 for World of Warcraft 12.1.0.69933 Live
Name             : Lava Burst (id=51505) [Spell Family (11)] 
Class            : Shaman
School           : Fire
Spell Type       : Magic
Range            : 40 yards
Cast Time        : 2 seconds
GCD              : 1.5 seconds
Velocity         : 50 yards/sec
Charges          : 1 (8 seconds cooldown)
Effects          :
#1 (id=43841)    : Trigger Missiles (32): Lava Burst
                   Base Value: 0 | Scaled Value: 0 | Trigger Spell: 285452 | Target: Targeted Enemy (6)
Description      : Hurls molten lava at the target, dealing $285452s1 Fire damage.

Name             : Lava Burst (id=285452) [Spell Family (11)] 
Class            : Shaman
School           : Fire
Effects          :
#1 (id=747255)   : School Damage (2): fire
                   Base Value: 0 | Scaled Value: 0 (delta=0.05) | SP Coefficient: 1.81 | Target: Targeted Enemy (6)
Description      : $@spelldesc51505
";

    #[test]
    fn parses_dump_block_and_resolves_missile_payload_coefficient() {
        let dump = SpellDump::parse_str(SAMPLE_DUMP);
        assert_eq!(dump.build.version, "12.1.0");
        assert_eq!(dump.build.build, 69933);
        let lvb = &dump.spells[&SpellId(51505)];
        assert_eq!(lvb.name, "Lava Burst");
        assert_eq!(lvb.cast_time_ms, Some(2000));
        assert_eq!(lvb.gcd_ms, Some(1500));
        assert_eq!(lvb.charges, Some((1, 8000)));
        assert_eq!(lvb.velocity, Some(50.0));

        let resolved = dump.resolve_direct_damage(SpellId(51505));
        assert_eq!(
            resolved,
            vec![(Coefficient::SpellPower(1.81), SpellId(285452))]
        );
    }
}
