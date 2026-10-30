//! SimC's generated item tables (`engine/dbc/generated/*.inc`): what an
//! equipped item, its bonus ids, enchant, and gems do.
//!
//! An item's effects are `item_effect` rows, by item or by an
//! `ITEM_BONUS_ADD_ITEM_EFFECT` bonus (embellishments, crafted procs). An
//! enchant (`spell_item_enchantment`) grants stats or a spell; a gem names
//! its enchant through its `gem_properties`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use portunus_core::{ItemId, SpellId};

use crate::dump::{DumpSpell, SimcEquippedSlot};
use crate::IngestError;

/// `item_spell_trigger_type`: when an item effect's spell applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemTrigger {
    /// Pressed, sharing the item's equip cooldown.
    OnUse,
    /// A passive while equipped (procs ride on it).
    OnEquip,
    ChanceOnHit,
    /// Pressed, without the equip cooldown.
    OnNoDelayUse,
    Other(i8),
}

impl ItemTrigger {
    fn from_raw(raw: i64) -> Self {
        match raw {
            0 => Self::OnUse,
            1 => Self::OnEquip,
            2 => Self::ChanceOnHit,
            5 => Self::OnNoDelayUse,
            r => Self::Other(i8::try_from(r).unwrap_or(-1)),
        }
    }
}

/// One `item_effect.inc` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemEffectRow {
    pub id: u32,
    pub spell: SpellId,
    /// `None` for effects only bonus ids add.
    pub item: Option<ItemId>,
    pub index: i8,
    pub trigger: ItemTrigger,
    pub cooldown_group: i16,
    /// Milliseconds; `None` when the effect has no cooldown of its own.
    pub cooldown_ms: Option<u32>,
    pub group_cooldown_ms: Option<u32>,
}

/// One property of an enchant (`ench_type`/`ench_amount`/`ench_prop`/
/// `ench_coeff`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnchantProp {
    /// A stat (`ITEM_MOD_*` in `item_mod`), worth `coefficient` times the
    /// enchant's scaling budget, or `amount` when it doesn't scale.
    Stat {
        item_mod: u32,
        amount: i32,
        coefficient: f64,
    },
    /// A passive spell while equipped.
    EquipSpell(SpellId),
    /// A proc on hit.
    CombatSpell(SpellId),
    UseSpell(SpellId),
    Other {
        kind: u32,
        prop: u32,
    },
}

/// One `spell_item_enchantment.inc` row.
#[derive(Debug, Clone, PartialEq)]
pub struct EnchantRow {
    pub id: u32,
    /// The gem item granting it, for gem enchants.
    pub gem: Option<ItemId>,
    /// SimC's scaling class (`-7` secondary rating, `-1` primary, ...), 0
    /// when it doesn't scale.
    pub scaling_class: i32,
    pub max_scaling_level: u32,
    pub props: Vec<EnchantProp>,
    /// Empty for the few unnamed rows.
    pub name: String,
}

/// One `embellishment_data.inc` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbellishmentRow {
    pub name: String,
    pub bonus_id: u32,
    /// The `item_effect` row it adds.
    pub effect_id: u32,
    pub spell: SpellId,
}

/// The fields of an `item_data.inc` row the item layer reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRow {
    pub id: ItemId,
    pub name: String,
    pub item_level: u32,
    pub quality: u32,
    pub inventory_type: u32,
    pub item_class: u32,
    pub item_subclass: u32,
    /// A gem's `gem_property_data` id, 0 for anything else.
    pub gem_properties: u32,
    pub item_set: u32,
}

/// One `rand_prop_points.inc` row: an item level's stat budgets by slot
/// category (0 is the trinket, weapon, and main armor category).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropPoints {
    pub item_level: u32,
    pub damage_replace_stat: f64,
    pub damage_secondary: f64,
    pub epic: [f64; 5],
    pub rare: [f64; 5],
    pub uncommon: [f64; 5],
}

/// Where an equipped effect came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GearSource {
    Item(ItemId),
    /// A bonus id adding an item effect (an embellishment, a crafted proc).
    Bonus(u32),
    Enchant(u32),
    /// A gem's enchant.
    Gem(ItemId),
}

/// What one piece of equipped gear contributes.
#[derive(Debug, Clone, PartialEq)]
pub enum GearEffect {
    Spell {
        source: GearSource,
        spell: SpellId,
        trigger: ItemTrigger,
        /// The level the spell's item-scaled values follow.
        item_level: u16,
        cooldown_ms: Option<u32>,
    },
    Stat {
        source: GearSource,
        item_mod: u32,
        amount: i32,
        coefficient: f64,
        scaling_class: i32,
    },
}

/// SimC's generated item tables.
#[derive(Debug, Clone, Default)]
pub struct ItemTables {
    pub effects: BTreeMap<u32, ItemEffectRow>,
    pub effects_by_item: BTreeMap<ItemId, Vec<u32>>,
    /// Bonus id to the `item_effect` rows it adds.
    pub bonus_effects: BTreeMap<u32, Vec<u32>>,
    pub enchants: BTreeMap<u32, EnchantRow>,
    pub embellishments: BTreeMap<u32, EmbellishmentRow>,
    /// Gem property id to its enchant.
    pub gem_enchants: BTreeMap<u32, u32>,
    pub items: BTreeMap<ItemId, ItemRow>,
    pub prop_points: BTreeMap<u32, PropPoints>,
    /// `__combat_ratings_mult_by_ilvl`, by [`RatingMultiplier`], indexed by
    /// item level minus one.
    pub rating_multipliers: [Vec<f64>; 4],
}

/// SimC's `combat_rating_multiplier_type`: which curve scales an item's
/// ratings with its level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingMultiplier {
    Armor = 0,
    Weapon = 1,
    Trinket = 2,
    Jewelry = 3,
}

impl RatingMultiplier {
    /// SimC's `item_combat_rating_type`, by `inventory_type`.
    pub fn of_inventory_type(inventory_type: u32) -> Option<Self> {
        Some(match inventory_type {
            2 | 11 => Self::Jewelry,
            12 => Self::Trinket,
            13 | 15 | 17 | 21 | 22 | 25 | 26 => Self::Weapon,
            1 | 3..=10 | 14 | 16 | 20 | 23 => Self::Armor,
            _ => return None,
        })
    }
}

/// The item an effect scales with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScalingItem {
    pub item_level: u32,
    /// 4 epic, 3 rare, 2 uncommon.
    pub quality: u32,
    pub multiplier: Option<RatingMultiplier>,
}

/// SimC's `PLAYER_SPECIAL_SCALE7`/`8`/`9` scaling classes.
const SCALE_SECONDARY_RATING: i32 = -7;
const SCALE_REPLACE_PRIMARY: i32 = -8;
const SCALE_REPLACE_SECONDARY: i32 = -9;

const ITEM_BONUS_ADD_ITEM_EFFECT: i64 = 23;

impl ItemTables {
    /// Reads the live (non-PTR) tables from `dir`, SimC's
    /// `engine/dbc/generated`.
    pub fn from_dir(dir: &Path) -> Result<Self, IngestError> {
        let read = |name: &str| {
            let path = dir.join(name);
            fs::read_to_string(&path)
                .map_err(|e| IngestError::Io(format!("{}: {e}", path.display())))
        };
        let mut t = Self::default();
        for row in rows(&read("item_effect.inc")?, "__item_effect_data") {
            let r = parse_item_effect(&row?)?;
            if let Some(item) = r.item {
                t.effects_by_item.entry(item).or_default().push(r.id);
            }
            t.effects.insert(r.id, r);
        }
        for row in rows(&read("item_bonus.inc")?, "__item_bonus_data") {
            let row = row?;
            let [_, bonus_id, kind, value, ..] = ints::<4>(&row, "item_bonus")?;
            if kind == ITEM_BONUS_ADD_ITEM_EFFECT {
                t.bonus_effects
                    .entry(to_u32(bonus_id))
                    .or_default()
                    .push(to_u32(value));
            }
        }
        for row in rows(
            &read("spell_item_enchantment.inc")?,
            "__spell_item_ench_data",
        ) {
            let r = parse_enchant(&row?)?;
            t.enchants.insert(r.id, r);
        }
        for row in rows(&read("embellishment_data.inc")?, "__embellishment_data") {
            let row = row?;
            let bad = || IngestError::Parse(format!("embellishment_data row: {row:?}"));
            let [Tok::Str(name), Tok::Num(bonus), Tok::Num(effect), Tok::Num(spell)] = &row[..]
            else {
                return Err(bad());
            };
            let bonus_id = num(bonus).map(to_u32).ok_or_else(bad)?;
            t.embellishments.insert(
                bonus_id,
                EmbellishmentRow {
                    name: name.clone(),
                    bonus_id,
                    effect_id: num(effect).map(to_u32).ok_or_else(bad)?,
                    spell: SpellId(num(spell).map(to_u32).ok_or_else(bad)?),
                },
            );
        }
        for row in rows(&read("gem_data.inc")?, "__gem_property_data") {
            let [id, enchant, ..] = ints::<2>(&row?, "gem_data")?;
            t.gem_enchants.insert(to_u32(id), to_u32(enchant));
        }
        let item_data = read("item_data.inc")?;
        for row in rows(&item_data, "__item_data_chunk") {
            let r = parse_item(&row?)?;
            t.items.insert(r.id, r);
        }
        for row in rows(&read("rand_prop_points.inc")?, "__rand_prop_points_data") {
            let row = row?;
            let v: Vec<f64> = row
                .iter()
                .map(|tok| match tok {
                    Tok::Num(s) => num_f64(s),
                    Tok::Str(_) => None,
                })
                .collect::<Option<_>>()
                .ok_or_else(|| IngestError::Parse(format!("rand_prop_points row: {row:?}")))?;
            let [lvl, replace, secondary, rest @ ..] = &v[..] else {
                return Err(IngestError::Parse(format!("rand_prop_points row: {row:?}")));
            };
            let band = |i: usize| -> [f64; 5] {
                std::array::from_fn(|k| rest.get(i * 5 + k).copied().unwrap_or(0.0))
            };
            let item_level = *lvl as u32;
            t.prop_points.insert(
                item_level,
                PropPoints {
                    item_level,
                    damage_replace_stat: *replace,
                    damage_secondary: *secondary,
                    epic: band(0),
                    rare: band(1),
                    uncommon: band(2),
                },
            );
        }
        t.rating_multipliers = parse_rating_multipliers(&read("sc_scale_data.inc")?)?;
        for (what, empty) in [
            ("item_effect", t.effects.is_empty()),
            ("spell_item_enchantment", t.enchants.is_empty()),
            ("gem_data", t.gem_enchants.is_empty()),
            ("item_data", t.items.is_empty()),
            ("rand_prop_points", t.prop_points.is_empty()),
        ] {
            if empty {
                return Err(IngestError::Parse(format!("{what}: no rows")));
            }
        }
        Ok(t)
    }

    /// SimC's `spelleffect_data_t::average` for an item's effect: values of
    /// spells that don't scale with their item as dumped (the player's
    /// level), others from the item's level budget. Effects without a
    /// scaling coefficient are their base value.
    pub fn effect_average(&self, spell: &DumpSpell, index: u8, item: Option<ScalingItem>) -> f64 {
        let Some(eff) = spell.effects.iter().find(|e| e.index == index) else {
            return 0.0;
        };
        let item = match item {
            Some(item) if spell.scales_with_item_level => item,
            _ => return eff.scaled_value.or(eff.base_value).unwrap_or(0.0),
        };
        let Some(coefficient) = eff.scaling_coefficient else {
            return eff.base_value.unwrap_or(0.0);
        };
        let level = match spell.max_scaling_level {
            Some(max) if max > 0 => item.item_level.min(max),
            _ => item.item_level,
        };
        let Some(props) = self.prop_points.get(&level) else {
            return 0.0;
        };
        let budget = match item.quality {
            q if q >= 4 => props.epic[0],
            3 => props.rare[0],
            _ => props.uncommon[0],
        };
        let budget = match eff.scaling_class {
            Some(SCALE_SECONDARY_RATING) => {
                budget
                    * item
                        .multiplier
                        .map_or(1.0, |m| self.rating_multiplier(m, level))
            }
            Some(SCALE_REPLACE_PRIMARY) => props.damage_replace_stat,
            Some(SCALE_REPLACE_SECONDARY) | None => props.damage_secondary,
            Some(_) => budget,
        };
        coefficient * budget
    }

    /// The combat rating multiplier at `item_level`, 1 off the table.
    pub fn rating_multiplier(&self, kind: RatingMultiplier, item_level: u32) -> f64 {
        let table = &self.rating_multipliers[kind as usize];
        item_level
            .checked_sub(1)
            .and_then(|i| table.get(i as usize))
            .copied()
            .filter(|&m| m != 0.0)
            .unwrap_or(1.0)
    }

    /// The scaling of an equipped slot's own item, once SimC's report has
    /// given its quality.
    pub fn scaling_item(&self, slot: &SimcEquippedSlot) -> ScalingItem {
        let row = self.items.get(&slot.item_id);
        ScalingItem {
            item_level: u32::from(slot.item_level),
            quality: slot.quality.or(row.map(|r| r.quality)).unwrap_or(4),
            multiplier: row.and_then(|r| RatingMultiplier::of_inventory_type(r.inventory_type)),
        }
    }

    /// Every effect an equipped slot carries: the item's own, its bonus ids',
    /// its enchant's, and its gems' enchants'. Unknown ids contribute
    /// nothing.
    pub fn equipped_effects(&self, slot: &SimcEquippedSlot) -> Vec<GearEffect> {
        let mut out = Vec::new();
        let spell_of = |source: GearSource, row: &ItemEffectRow| GearEffect::Spell {
            source,
            spell: row.spell,
            trigger: row.trigger,
            item_level: slot.item_level,
            cooldown_ms: row.cooldown_ms,
        };
        for id in self
            .effects_by_item
            .get(&slot.item_id)
            .into_iter()
            .flatten()
        {
            if let Some(row) = self.effects.get(id) {
                out.push(spell_of(GearSource::Item(slot.item_id), row));
            }
        }
        for &bonus in &slot.bonus_ids {
            for id in self.bonus_effects.get(&bonus).into_iter().flatten() {
                if let Some(row) = self.effects.get(id) {
                    out.push(spell_of(GearSource::Bonus(bonus), row));
                }
            }
        }
        if let Some(e) = slot.enchant_id {
            self.enchant_effects(GearSource::Enchant(e), e, slot.item_level, &mut out);
        }
        for &gem in &slot.gems {
            let enchant = self
                .items
                .get(&gem)
                .and_then(|g| self.gem_enchants.get(&g.gem_properties));
            if let Some(&e) = enchant {
                self.enchant_effects(GearSource::Gem(gem), e, slot.item_level, &mut out);
            }
        }
        out
    }

    fn enchant_effects(
        &self,
        source: GearSource,
        enchant: u32,
        item_level: u16,
        out: &mut Vec<GearEffect>,
    ) {
        let Some(row) = self.enchants.get(&enchant) else {
            return;
        };
        for prop in &row.props {
            match *prop {
                EnchantProp::Stat {
                    item_mod,
                    amount,
                    coefficient,
                } => out.push(GearEffect::Stat {
                    source,
                    item_mod,
                    amount,
                    coefficient,
                    scaling_class: row.scaling_class,
                }),
                EnchantProp::EquipSpell(spell) | EnchantProp::CombatSpell(spell) => {
                    out.push(GearEffect::Spell {
                        source,
                        spell,
                        trigger: ItemTrigger::OnEquip,
                        item_level,
                        cooldown_ms: None,
                    });
                }
                EnchantProp::UseSpell(spell) => out.push(GearEffect::Spell {
                    source,
                    spell,
                    trigger: ItemTrigger::OnUse,
                    item_level,
                    cooldown_ms: None,
                }),
                EnchantProp::Other { .. } => {}
            }
        }
    }
}

/// A token of a C initializer row, braces flattened away.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Str(String),
    /// A number or any other bare word (`0x7f`, `1.5f`,
    /// `&__item_stats_data[2]`).
    Num(String),
}

/// The rows of every array whose name starts with `array`, each a
/// flattened token list.
fn rows<'a>(
    text: &'a str,
    array: &'a str,
) -> impl Iterator<Item = Result<Vec<Tok>, IngestError>> + 'a {
    let mut inside = false;
    text.lines().filter_map(move |line| {
        let trimmed = line.trim();
        if !inside {
            inside = trimmed.starts_with("static") && trimmed.contains(array);
            return None;
        }
        if trimmed.starts_with("} }") || trimmed.starts_with("};") {
            inside = false;
            return None;
        }
        trimmed.starts_with('{').then(|| tokenize(trimmed))
    })
}

fn tokenize(line: &str) -> Result<Vec<Tok>, IngestError> {
    let body = line.split("//").next().unwrap_or(line);
    let mut out = Vec::new();
    let mut chars = body.chars().peekable();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<Tok>| {
        if !word.is_empty() {
            out.push(Tok::Num(std::mem::take(word)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                flush(&mut word, &mut out);
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some('\\') => {
                            if let Some(esc) = chars.next() {
                                s.push(esc);
                            }
                        }
                        Some('"') => break,
                        Some(ch) => s.push(ch),
                        None => {
                            return Err(IngestError::Parse(format!("unterminated string: {line}")))
                        }
                    }
                }
                out.push(Tok::Str(s));
            }
            '{' | '}' | ',' => flush(&mut word, &mut out),
            c if c.is_whitespace() => flush(&mut word, &mut out),
            c => word.push(c),
        }
    }
    flush(&mut word, &mut out);
    Ok(out)
}

fn num(s: &str) -> Option<i64> {
    if let Some(hex) = s.strip_prefix("0x") {
        return i64::from_str_radix(hex, 16)
            .ok()
            .or_else(|| u64::from_str_radix(hex, 16).ok().map(|v| v as i64));
    }
    s.parse().ok()
}

fn num_f64(s: &str) -> Option<f64> {
    s.trim_end_matches('f').parse().ok()
}

fn to_u32(v: i64) -> u32 {
    u32::try_from(v.max(0)).unwrap_or(u32::MAX)
}

/// The first `N` tokens of a row, as integers.
fn ints<const N: usize>(row: &[Tok], what: &str) -> Result<[i64; N], IngestError> {
    let bad = || IngestError::Parse(format!("{what} row: {row:?}"));
    let mut out = [0; N];
    for (slot, tok) in out.iter_mut().zip(row) {
        let Tok::Num(s) = tok else {
            return Err(bad());
        };
        *slot = num(s).ok_or_else(bad)?;
    }
    if row.len() < N {
        return Err(bad());
    }
    Ok(out)
}

/// The four item-level curves of `__combat_ratings_mult_by_ilvl` (armor,
/// weapon, trinket, jewelry), each introduced by a `// ... Multiplier` line.
fn parse_rating_multipliers(text: &str) -> Result<[Vec<f64>; 4], IngestError> {
    let mut blocks: Vec<Vec<f64>> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if !inside {
            inside = line.contains("__combat_ratings_mult_by_ilvl");
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("};") {
            break;
        }
        if trimmed.starts_with("//") && trimmed.contains("Multiplier") {
            blocks.push(Vec::new());
            continue;
        }
        let Some(block) = blocks.last_mut() else {
            continue;
        };
        let body = trimmed.split("//").next().unwrap_or(trimmed);
        for v in body.split([',', ' ', '\t']).filter(|s| !s.is_empty()) {
            if let Ok(v) = v.parse::<f64>() {
                block.push(v);
            }
        }
    }
    let bad = || IngestError::Parse("sc_scale_data: combat rating multipliers".into());
    let [armor, weapon, trinket, jewelry]: [Vec<f64>; 4] = blocks.try_into().map_err(|_| bad())?;
    if armor.is_empty() || weapon.len() != armor.len() {
        return Err(bad());
    }
    Ok([armor, weapon, trinket, jewelry])
}

fn parse_item_effect(row: &[Tok]) -> Result<ItemEffectRow, IngestError> {
    let [id, spell, item, index, trigger, group, cd, group_cd] = ints::<8>(row, "item_effect")?;
    let cooldown = |ms: i64| u32::try_from(ms).ok().filter(|&ms| ms > 0);
    Ok(ItemEffectRow {
        id: to_u32(id),
        spell: SpellId(to_u32(spell)),
        item: (item > 0).then(|| ItemId(to_u32(item))),
        index: i8::try_from(index).unwrap_or(-1),
        trigger: ItemTrigger::from_raw(trigger),
        cooldown_group: i16::try_from(group).unwrap_or(0),
        cooldown_ms: cooldown(cd),
        group_cooldown_ms: cooldown(group_cd),
    })
}

/// `{id, id_gem, id_scaling, min_scaling_level, max_scaling_level,
/// min_ilevel, max_ilevel, req_skill, req_skill_value, ench_type[3],
/// ench_amount[3], ench_prop[3], ench_coeff[3], id_spell, name}`.
fn parse_enchant(row: &[Tok]) -> Result<EnchantRow, IngestError> {
    let bad = || IngestError::Parse(format!("spell_item_enchantment row: {row:?}"));
    let name = match row.last() {
        Some(Tok::Str(name)) => name.clone(),
        _ => String::new(),
    };
    let [id, gem, scaling, _min_scaling, max_scaling, _min_ilvl, _max_ilvl, _skill, _skill_value, t0, t1, t2, a0, a1, a2, p0, p1, p2] =
        ints::<18>(row, "spell_item_enchantment")?;
    let coeff = |i: usize| match row.get(18 + i) {
        Some(Tok::Num(s)) => num_f64(s).ok_or_else(bad),
        _ => Err(bad()),
    };
    let coeffs = [coeff(0)?, coeff(1)?, coeff(2)?];
    let mut props = Vec::new();
    for (k, ((kind, amount), prop)) in [t0, t1, t2]
        .into_iter()
        .zip([a0, a1, a2])
        .zip([p0, p1, p2])
        .enumerate()
    {
        let spell = SpellId(to_u32(prop));
        props.push(match kind {
            0 => continue,
            1 => EnchantProp::CombatSpell(spell),
            3 => EnchantProp::EquipSpell(spell),
            5 => EnchantProp::Stat {
                item_mod: to_u32(prop),
                amount: i32::try_from(amount).unwrap_or(0),
                coefficient: coeffs[k],
            },
            7 => EnchantProp::UseSpell(spell),
            other => EnchantProp::Other {
                kind: to_u32(other),
                prop: to_u32(prop),
            },
        });
    }
    Ok(EnchantRow {
        id: to_u32(id),
        gem: (gem > 0).then(|| ItemId(to_u32(gem))),
        scaling_class: i32::try_from(scaling).unwrap_or(0),
        max_scaling_level: to_u32(max_scaling),
        props,
        name,
    })
}

/// `{name, id, flags_1, flags_2, type_flags, level, req_level, req_skill,
/// req_skill_level, quality, inventory_type, item_class, item_subclass,
/// bind_type, delay, dmg_range, item_modifier, stats, stats_count,
/// class_mask, race_mask, socket_color[3], gem_properties, id_socket_bonus,
/// id_set, ...}`.
fn parse_item(row: &[Tok]) -> Result<ItemRow, IngestError> {
    let bad = || IngestError::Parse(format!("item_data row: {row:?}"));
    let Some(Tok::Str(name)) = row.first() else {
        return Err(bad());
    };
    let field = |i: usize| match row.get(i) {
        Some(Tok::Num(s)) => num(s).map(to_u32).ok_or_else(bad),
        _ => Err(bad()),
    };
    Ok(ItemRow {
        id: ItemId(field(1)?),
        name: name.clone(),
        item_level: field(5)?,
        quality: field(9)?,
        inventory_type: field(10)?,
        item_class: field(11)?,
        item_subclass: field(12)?,
        gem_properties: field(24)?,
        item_set: field(26)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_escaped_names_and_nested_braces() {
        let toks =
            tokenize(r#"{ "BKP 42 \"Ultra\"", 3025, 0x10, { 0, 1, 2 }, 0.6f }, // x"#).unwrap();
        assert_eq!(toks[0], Tok::Str("BKP 42 \"Ultra\"".into()));
        assert_eq!(toks.len(), 7);
        assert_eq!(num("0x10"), Some(16));
        assert_eq!(num_f64("0.6f"), Some(0.6));
    }
}
