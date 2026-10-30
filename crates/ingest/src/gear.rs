//! A SimC profile's gear as real item effects: weapon enchants, embellishments,
//! and item procs, translated into auras, listeners, and spells.
//!
//! Like SimC's `register_special_effect`, each effect is keyed by its driver
//! spell; SimC hand-writes most Midnight effects, so this does too, reading
//! every value from the spell data at the item's level. A driver without a
//! translation is reported instead of approximated. Stats from gems and
//! enchants are left out: they're in the profile's gear totals.

use std::collections::BTreeMap;
use std::path::Path;

use portunus_core::{AuraId, SimDuration, SpecId, SpellId};
use portunus_gamedata::aura::{AuraDef, RefreshRule};
use portunus_gamedata::effect::{
    Coefficient, Effect, EffectTarget, ListenFor, Listener, ModKind, ModScope, Modifier, ProcChance,
};
use portunus_gamedata::item::GearSlot;
use portunus_gamedata::spell::{CastKind, SpellDef, Targeting};
use portunus_gamedata::stats::{SchoolMask, Stat};
use portunus_gamedata::GameData;

use crate::dump::{simc_rating_scale, simple_passive_aura, DumpEffect, DumpSpell, SimcProfile};
use crate::items::{GearEffect, GearSource, ItemTables, ScalingItem};
use crate::{IngestError, SpellDump};

/// SimC's item tables and item spells, loaded once for any number of
/// profiles.
#[derive(Debug, Clone)]
pub struct GearLibrary {
    pub tables: ItemTables,
    pub spells: SpellDump,
}

/// What a profile's gear became.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EquippedGear {
    /// The passive auras each slot grants.
    pub auras: BTreeMap<GearSlot, Vec<AuraId>>,
    /// Effects with no translation yet.
    pub unmodelled: Vec<UnmodelledEffect>,
}

/// A gear effect left out of the simulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmodelledEffect {
    pub slot: GearSlot,
    pub source: GearSource,
    pub spell: SpellId,
    pub name: String,
}

/// A stat buff a proc applies, and what it grants.
struct BuffGrant {
    buff: SpellId,
    stats: Vec<(Stat, f64)>,
}

/// Translations made for one profile, merged as SimC merges a second copy
/// of the same effect: into the first one's buff and passive.
struct Build<'a> {
    lib: &'a GearLibrary,
    primary: Stat,
    auras: BTreeMap<AuraId, AuraDef>,
    spells: BTreeMap<SpellId, SpellDef>,
}

const NECROTIC_TEAR: SpellId = SpellId(1305391);
const BREATH_OF_CORRUPTION: SpellId = SpellId(1305395);
const ARCANOWEAVE_INSIGHT: SpellId = SpellId(1229746);
const HUNTERS_RITUALS: [SpellId; 4] = [
    SpellId(1295898),
    SpellId(1295899),
    SpellId(1295900),
    SpellId(1295901),
];

impl GearLibrary {
    /// Loads from a SimC checkout (`engine/dbc/generated` and
    /// `SpellDataDump/nonclass.txt`).
    pub fn from_simc_dir(simc: &Path) -> Result<Self, IngestError> {
        Ok(Self {
            tables: ItemTables::from_dir(&simc.join("engine/dbc/generated"))?,
            spells: SpellDump::nonclass_from_dir(&simc.join("SpellDataDump"))?,
        })
    }

    /// Adds the effects of `profile`'s gear to `data` for a seat of `spec`.
    /// Trinkets' own effects are left to the profile's trinket handling.
    pub fn equip(&self, data: &mut GameData, profile: &SimcProfile, spec: SpecId) -> EquippedGear {
        let primary = data
            .specs
            .get(&spec)
            .map_or(Stat::Agility, |s| s.primary_stat);
        let mut build = Build {
            lib: self,
            primary,
            auras: BTreeMap::new(),
            spells: BTreeMap::new(),
        };
        let mut out = EquippedGear::default();
        for (&slot, equipped) in &profile.slots {
            let item = self.tables.scaling_item(equipped);
            for effect in self.tables.equipped_effects(equipped) {
                let GearEffect::Spell { source, spell, .. } = effect else {
                    continue;
                };
                let trinket = matches!(slot, GearSlot::Trinket1 | GearSlot::Trinket2);
                if trinket && matches!(source, GearSource::Item(_)) {
                    continue;
                }
                match build.translate(spell, item) {
                    Some(Some(aura)) => out.auras.entry(slot).or_default().push(aura),
                    Some(None) => {}
                    None => out.unmodelled.push(UnmodelledEffect {
                        slot,
                        source,
                        spell,
                        name: self
                            .spells
                            .spells
                            .get(&spell)
                            .map_or_else(String::new, |s| s.name.clone()),
                    }),
                }
            }
        }
        data.auras.extend(build.auras);
        data.spells.extend(build.spells);
        out
    }
}

impl Build<'_> {
    fn spell(&self, id: SpellId) -> Option<&DumpSpell> {
        self.lib.spells.spells.get(&id)
    }

    fn average(&self, spell: SpellId, index: u8, item: ScalingItem) -> f64 {
        self.spell(spell).map_or(0.0, |s| {
            self.lib.tables.effect_average(s, index, Some(item))
        })
    }

    fn base(&self, spell: SpellId, index: u8) -> f64 {
        self.spell(spell)
            .and_then(|s| effect(s, index))
            .and_then(|e| e.base_value)
            .unwrap_or(0.0)
    }

    fn trigger(&self, spell: SpellId, index: u8) -> Option<SpellId> {
        self.spell(spell)
            .and_then(|s| effect(s, index))
            .and_then(|e| e.trigger_spell)
    }

    /// The stats effect `index` of `buff` grants.
    fn buff_stats(&self, buff: SpellId, index: u8) -> Vec<Stat> {
        self.spell(buff)
            .and_then(|s| effect(s, index))
            .map_or_else(Vec::new, |e| effect_stats(e, self.primary))
    }

    /// The passive aura `driver` grants: `Some(Some(aura))` the first time,
    /// `Some(None)` when merged into an earlier copy, `None` untranslated.
    fn translate(&mut self, driver: SpellId, item: ScalingItem) -> Option<Option<AuraId>> {
        match driver.0 {
            // SimC's `stat_weapon_enchant`: the driver's value on the buff its
            // RPPM spell procs.
            1236741 | 1236742 | 1236727 | 1236728 | 1236712 | 1236721 | 1236724 | 1236725
            | 1236729 | 1236730 => {
                let rppm = self.trigger(driver, 1)?;
                let buff = self.trigger(rppm, 1)?;
                let amount = self.average(driver, 1, item);
                let stats = self.buff_stats(buff, 1);
                Some(self.stat_proc(rppm, vec![grant(buff, &stats, amount)]))
            }
            // Venomcursed: the driver procs its buff, effect for effect.
            1307906 | 1307923 | 1307928 => {
                let buff = self.trigger(driver, 1)?;
                let mut stats = Vec::new();
                for index in 1..=2 {
                    let amount = self.average(driver, index, item);
                    stats.extend(
                        self.buff_stats(buff, index)
                            .into_iter()
                            .map(|s| (s, amount)),
                    );
                }
                Some(self.stat_proc(driver, vec![BuffGrant { buff, stats }]))
            }
            // Arcanoweave Lining: its effect #2 share of the value goes to the
            // wearer.
            1283697 => {
                let rppm = self.trigger(driver, 1)?;
                let amount = self.average(driver, 1, item) * self.base(driver, 2) / 100.0;
                let stats = self.buff_stats(ARCANOWEAVE_INSIGHT, 1);
                Some(self.stat_proc(rppm, vec![grant(ARCANOWEAVE_INSIGHT, &stats, amount)]))
            }
            // Hunter's Ritual Stone: one of four rating rituals at random.
            1297382 => {
                let rppm = self.trigger(driver, 1)?;
                let amount = self.average(driver, 2, item);
                let grants = HUNTERS_RITUALS
                    .iter()
                    .map(|&buff| grant(buff, &self.buff_stats(buff, 1), amount))
                    .collect();
                Some(self.stat_proc(rppm, grants))
            }
            // Eyes of the Eagle: a passive crit damage bonus; copies add.
            1236700 | 1236701 => {
                let value = self.base(driver, 1);
                let modifier = all(ModKind::CritDamagePct, value);
                Some(self.passive(driver, vec![modifier], Vec::new()))
            }
            // Zatha'tek: Breath of Corruption, whose Necrotic Tears on the
            // target add to its later hits.
            1298023 => {
                let rppm = self.spell(driver)?.rppm?;
                let amount = self.average(driver, 1, item);
                let per_tear = self.base(driver, 2);
                self.zathatek(amount, per_tear);
                let listener = rppm_listener(rppm, vec![trigger(BREATH_OF_CORRUPTION)]);
                Some(self.passive(driver, Vec::new(), vec![listener]))
            }
            _ => None,
        }
    }

    /// A passive driver aura procs `grants` (one at random when several)
    /// at its RPPM spell's rate. A second copy adds its stats to the first's
    /// buffs.
    fn stat_proc(&mut self, rppm: SpellId, grants: Vec<BuffGrant>) -> Option<AuraId> {
        let mut effects = Vec::new();
        for g in &grants {
            self.stat_buff(g);
            effects.push(Effect::ApplyAura {
                aura: AuraId(g.buff.0),
                target: EffectTarget::Caster,
                stacks: 1,
                duration: None,
                per_unit_spent: None,
            });
        }
        let driver = AuraId(rppm.0);
        if self.auras.contains_key(&driver) {
            return None;
        }
        let effects = if effects.len() == 1 {
            effects
        } else {
            let share = 1.0 / effects.len() as f64;
            vec![Effect::RandomOf(
                effects.into_iter().map(|e| (share, e)).collect(),
            )]
        };
        let rate = self
            .spell(rppm)
            .and_then(|s| s.rppm)
            .unwrap_or((1.0, false));
        let listener = rppm_listener(rate, effects);
        let name = self.name(rppm);
        self.auras.insert(
            driver,
            simple_passive_aura(driver, &name, Vec::new(), vec![listener]),
        );
        Some(driver)
    }

    fn stat_buff(&mut self, g: &BuffGrant) {
        let id = AuraId(g.buff.0);
        let modifiers: Vec<Modifier> = g
            .stats
            .iter()
            .map(|&(stat, v)| all(ModKind::StatFlat(stat), v * simc_rating_scale(stat)))
            .collect();
        if let Some(existing) = self.auras.get_mut(&id) {
            merge(&mut existing.modifiers, modifiers);
            return;
        }
        let name = self.name(g.buff);
        let mut aura = simple_passive_aura(id, &name, modifiers, Vec::new());
        aura.duration = self
            .spell(g.buff)
            .and_then(|s| s.duration_ms)
            .map(SimDuration);
        self.auras.insert(id, aura);
    }

    /// A passive aura for `driver`, or `modifiers` added to the one an
    /// earlier copy made.
    fn passive(
        &mut self,
        driver: SpellId,
        modifiers: Vec<Modifier>,
        listeners: Vec<Listener>,
    ) -> Option<AuraId> {
        let id = AuraId(driver.0);
        if let Some(existing) = self.auras.get_mut(&id) {
            merge(&mut existing.modifiers, modifiers);
            return None;
        }
        let name = self.name(driver);
        self.auras
            .insert(id, simple_passive_aura(id, &name, modifiers, listeners));
        Some(id)
    }

    fn zathatek(&mut self, amount: f64, per_tear: f64) {
        let tear = AuraId(NECROTIC_TEAR.0);
        let mut debuff = simple_passive_aura(
            tear,
            &self.name(NECROTIC_TEAR),
            vec![Modifier {
                scope: ModScope::Spell(BREATH_OF_CORRUPTION),
                kind: ModKind::SourceDamageTakenPct,
                value: per_tear,
                per_stack: true,
                condition: None,
            }],
            Vec::new(),
        );
        debuff.max_stacks = self
            .spell(NECROTIC_TEAR)
            .and_then(|s| s.max_stacks)
            .unwrap_or(1);
        debuff.refresh = RefreshRule::Replace;
        self.auras.insert(tear, debuff);
        let school = self
            .spell(BREATH_OF_CORRUPTION)
            .and_then(|s| s.school.as_deref())
            .map_or(SchoolMask::NATURE, school_mask);
        let mut damage = item_spell(
            BREATH_OF_CORRUPTION,
            &self.name(BREATH_OF_CORRUPTION),
            school,
        );
        damage.effects = vec![
            Effect::Damage {
                amount: Coefficient::Flat(amount),
                school,
                target: EffectTarget::Target,
                aoe: None,
                hand: None,
                ignores_armor: false,
                per_count: None,
                unmodified: false,
            },
            Effect::ApplyAura {
                aura: tear,
                target: EffectTarget::Target,
                stacks: 1,
                duration: None,
                per_unit_spent: None,
            },
        ];
        self.spells.insert(BREATH_OF_CORRUPTION, damage);
    }

    fn name(&self, spell: SpellId) -> String {
        self.spell(spell)
            .map_or_else(|| format!("Item effect {}", spell.0), |s| s.name.clone())
    }
}

fn effect(spell: &DumpSpell, index: u8) -> Option<&DumpEffect> {
    spell.effects.iter().find(|e| e.index == index)
}

fn grant(buff: SpellId, stats: &[Stat], amount: f64) -> BuffGrant {
    BuffGrant {
        buff,
        stats: stats.iter().map(|&s| (s, amount)).collect(),
    }
}

fn all(kind: ModKind, value: f64) -> Modifier {
    Modifier {
        scope: ModScope::All,
        kind,
        value,
        per_stack: false,
        condition: None,
    }
}

/// Adds `more` into `into`, summing modifiers of the same scope and kind.
fn merge(into: &mut Vec<Modifier>, more: Vec<Modifier>) {
    for m in more {
        match into
            .iter_mut()
            .find(|x| x.scope == m.scope && x.kind == m.kind && x.per_stack == m.per_stack)
        {
            Some(x) => x.value += m.value,
            None => into.push(m),
        }
    }
}

fn trigger(spell: SpellId) -> Effect {
    Effect::TriggerSpell {
        spell,
        target: EffectTarget::Target,
        delay: SimDuration::ZERO,
    }
}

/// Real-PPM `effects` off the holder's hits.
fn rppm_listener((rate, hasted): (f64, bool), effects: Vec<Effect>) -> Listener {
    Listener {
        on: ListenFor::DamageDealt {
            spell: None,
            school: None,
            crit_only: false,
            killing_blow: false,
            direct: None,
            aura: None,
            class_only: false,
        },
        chance: ProcChance::Rppm {
            rate,
            hasted,
            crit: false,
        },
        internal_cooldown: None,
        per_unit: false,
        shared_with: None,
        condition: None,
        chain: false,
        effects,
    }
}

/// An instant item spell with no effects yet.
fn item_spell(id: SpellId, name: &str, school: SchoolMask) -> SpellDef {
    SpellDef {
        id,
        name: name.into(),
        school,
        cast: CastKind::Instant,
        gcd: None,
        cooldown: None,
        costs: Vec::new(),
        targeting: Targeting::Enemy,
        hostile: true,
        range: Some(100.0),
        speed: None,
        min_travel: SimDuration::ZERO,
        rolls_on_impact: false,
        castable_while_moving: true,
        usable_while_casting: true,
        weapon: None,
        unrecorded: false,
        item: true,
        no_procs: false,
        requires: Vec::new(),
        on_cast: Vec::new(),
        effects: Vec::new(),
    }
}

/// The stats a rating or attribute effect grants, `StrAgiInt` and its kin
/// as the spec's primary stat when they include it.
fn effect_stats(e: &DumpEffect, primary: Stat) -> Vec<Stat> {
    if let Some(ratings) = &e.rating {
        return ratings
            .split(',')
            .filter_map(|r| match r.trim() {
                "Haste" => Some(Stat::HasteRating),
                "Crit" => Some(Stat::CritRating),
                "Mastery" => Some(Stat::MasteryRating),
                "Vers" => Some(Stat::VersatilityRating),
                _ => None,
            })
            .collect();
    }
    let Some(stat) = e.stat.as_deref() else {
        return Vec::new();
    };
    let has = |abbrev: &str| stat.contains(abbrev);
    let primary_in = match primary {
        Stat::Strength => has("Str"),
        Stat::Agility => has("Agi"),
        Stat::Intellect => has("Int"),
        _ => false,
    };
    match stat {
        "Sta" => vec![Stat::Stamina],
        _ if primary_in => vec![primary],
        _ => Vec::new(),
    }
}

fn school_mask(name: &str) -> SchoolMask {
    match name {
        "Holy" => SchoolMask::HOLY,
        "Fire" => SchoolMask::FIRE,
        "Nature" => SchoolMask::NATURE,
        "Frost" => SchoolMask::FROST,
        "Shadow" => SchoolMask::SHADOW,
        "Arcane" => SchoolMask::ARCANE,
        _ => SchoolMask::PHYSICAL,
    }
}
