//! Aura modifiers read from a spell's own effects in `SpellDataDump`.
//!
//! The effect type fixes what a modifier reaches, as in the game: a
//! school-scoped effect ("Modify Damage Done%", "Modify Damage Taken%")
//! reaches every spell of its schools, item effects included; a spell-family
//! or label modifier ("Add Percent Modifier", "Apply Percent Modifier w/
//! Label", "Modify Damage Taken% from Caster's Spells") reaches exactly the
//! spells it lists, which the dump spells out for both.
//!
//! An effect's value can be changed by other spells' effects (`Modified
//! By:`), e.g. a spec's tuning or a talent raising a buff's bonus. Each
//! such change becomes a variant conditioned on the caster holding the
//! changing aura.

use std::collections::BTreeMap;

use portunus_core::{AuraId, SpellId};
use portunus_gamedata::effect::{ModKind, ModScope, Modifier, Predicate};
use portunus_gamedata::stats::SchoolMask;
use portunus_gamedata::talent::Grant;
use portunus_gamedata::{GameBuild, GameData};
use serde::{Deserialize, Serialize};

use crate::dump::{spec_passive_aura_id, DumpEffect, DumpSpell, SpellDump};

/// Every school, as a `Misc Value` mask.
const ALL_SCHOOLS: u32 = 0x7f;

/// The modifier kinds [`derive_aura`] produces; a hand-written modifier of
/// another kind is out of its reach.
pub const DERIVED_KINDS: [ModKind; 10] = [
    ModKind::DamageDonePct,
    ModKind::DirectDamagePct,
    ModKind::PeriodicDamagePct,
    ModKind::MasteryDamagePct,
    ModKind::CritChanceAdd,
    ModKind::CritDamagePct,
    ModKind::SourceDamageTakenPct,
    ModKind::DamageTakenPct,
    ModKind::PetDamagePct,
    ModKind::GuardianDamagePct,
];

/// What the data around an aura offers [`derive_aura`].
pub struct DeriveInput<'a> {
    pub dump: &'a SpellDump,
    /// Spells the data defines: the listed spells a modifier keeps.
    pub known_spell: &'a dyn Fn(SpellId) -> bool,
    /// Auras a caster can hold in the data: what a variant's condition can
    /// name.
    pub known_aura: &'a dyn Fn(AuraId) -> bool,
    /// How many of the aura's ranks the data's talents grant, each as a
    /// stack; `None` for an aura no talent grants, which holds every rank.
    pub rank_grants: Option<u8>,
}

/// Auras that hold one effect per spec, chosen by the tooltip's `$?c`
/// rather than by the effects: their hand-written modifiers carry the spec
/// conditions the dump can't.
pub const SPEC_SELECTED: [AuraId; 1] = [AuraId(457273)];

/// A modifier read from one of a spell's effects.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedModifier {
    /// The effect's `#` index.
    pub effect: u8,
    pub modifier: Modifier,
}

/// An aura's derived modifiers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DerivedAura {
    pub modifiers: Vec<DerivedModifier>,
    /// Stacks the aura needs: its ranks, when each rank adds the first's
    /// value.
    pub max_stacks: Option<u8>,
    /// What the derivation approximated or left out.
    pub notes: Vec<String>,
}

/// What an effect's type reaches.
enum Reach {
    /// The spells the effect lists, by family mask or label.
    Listed,
    /// Every spell of the schools in its `Misc Value`.
    Schools,
    AutoAttack,
    All,
}

/// The modifier kind and reach of an effect, for the effect types that
/// adjust damage.
fn classify(kind: &str) -> Option<(ModKind, Reach)> {
    let percent = kind.contains("Add Percent Modifier (108)")
        || kind.contains("Apply Percent Modifier w/ Label (218)");
    let flat = kind.contains("Add Flat Modifier (107)")
        || kind.contains("Apply Flat Modifier w/ Label (219)");
    if percent && kind.contains("Spell Direct Amount (0)") {
        Some((ModKind::DirectDamagePct, Reach::Listed))
    } else if percent && kind.contains("Spell Periodic Amount (22)") {
        Some((ModKind::PeriodicDamagePct, Reach::Listed))
    } else if percent && kind.contains("Spell Critical Bonus Multiplier (15)") {
        Some((ModKind::CritDamagePct, Reach::Listed))
    } else if flat && kind.contains("Spell Critical Chance (7)") {
        Some((ModKind::CritChanceAdd, Reach::Listed))
    } else if kind.contains("Modify Damage Taken% from Caster's Spells (271)") {
        Some((ModKind::SourceDamageTakenPct, Reach::Listed))
    } else if kind.contains("Modify Damage Done% (79)") {
        Some((ModKind::DamageDonePct, Reach::Schools))
    } else if kind.contains("Modify Damage Taken% (87)") {
        Some((ModKind::DamageTakenPct, Reach::Schools))
    } else if kind.contains("Modify Auto Attack Damage Done% (344)") {
        Some((ModKind::DamageDonePct, Reach::AutoAttack))
    } else if kind.contains("Modify Critical Strike% (290)") {
        Some((ModKind::CritChanceAdd, Reach::All))
    } else if kind.contains("Modify Pet Damage Done% (429)") {
        Some((ModKind::PetDamagePct, Reach::All))
    } else if kind.contains("Modify Guardian Damage Done% (531)") {
        Some((ModKind::GuardianDamagePct, Reach::All))
    } else {
        None
    }
}

fn value(effect: &DumpEffect) -> f64 {
    effect.scaled_value.or(effect.base_value).unwrap_or(0.0)
}

/// How another spell's effect changes one of ours.
#[derive(Debug, Clone, Copy)]
enum Change {
    Add(f64),
    Scale(f64),
}

impl Change {
    fn apply(self, base: f64) -> f64 {
        match self {
            Self::Add(v) => base + v,
            Self::Scale(v) => base * (1.0 + v / 100.0),
        }
    }
}

/// The change `source`'s effect `index` makes to effect `ours` of `aura`,
/// when it adjusts that effect's value.
fn change(
    dump: &SpellDump,
    (source, index): (SpellId, u8),
    aura: SpellId,
    ours: u8,
) -> Option<Change> {
    let effect = dump
        .spells
        .get(&source)?
        .effects
        .iter()
        .find(|e| e.index == index)?;
    if !effect.kind.contains(&format!("Spell Effect {ours} ("))
        || !effect.affected_spells.contains(&aura)
    {
        return None;
    }
    let v = value(effect);
    if v.abs() < 1e-9 {
        return None;
    }
    if effect.kind.contains("Percent Modifier") {
        Some(Change::Scale(v))
    } else if effect.kind.contains("Flat Modifier") {
        Some(Change::Add(v))
    } else {
        None
    }
}

/// The modifiers `spell`'s effects give its aura. Listed spells the data
/// doesn't know are left out. A mastery's effects carry no value of their
/// own; they scale with mastery relative to its first coefficient.
pub fn derive_aura(spell: &DumpSpell, input: &DeriveInput<'_>) -> DerivedAura {
    let mut out = DerivedAura::default();
    let ranked = spell.max_rank.filter(|&r| r > 1);
    let stacking = spell.max_stacks.is_some_and(|n| n > 1);
    let mut all_ranks = None;
    if let Some(ranks) = ranked {
        match input.rank_grants {
            Some(n) if n == ranks => out.max_stacks = Some(ranks),
            Some(n) => out
                .notes
                .push(format!("{ranks} ranks, granted {n} times: rank 1 values")),
            None => all_ranks = Some(ranks),
        }
    }
    let mastery_base = spell
        .name
        .starts_with("Mastery:")
        .then(|| spell.effects.iter().find_map(|e| e.sp_coefficient))
        .flatten()
        .filter(|c| *c > 0.0);
    let mut raw: Vec<DerivedModifier> = Vec::new();
    for effect in &spell.effects {
        let Some((mut kind, reach)) = classify(&effect.kind) else {
            continue;
        };
        let mut base = value(effect);
        if let (Some(m), Some(coef)) = (mastery_base, effect.sp_coefficient) {
            if base.abs() < 1e-9
                && matches!(kind, ModKind::DirectDamagePct | ModKind::PeriodicDamagePct)
            {
                kind = ModKind::MasteryDamagePct;
                base = coef / m;
            }
        }
        let per_stack = match ranked {
            Some(_) => {
                let scaling = spell.rank_scaling.iter().find(|r| r.effect == effect.index);
                let linear = scaling.is_some_and(|r| r.linear);
                if scaling.is_some() && !linear {
                    out.notes.push(format!(
                        "#{} rank values not linear: rank 1 value",
                        effect.index
                    ));
                }
                if let (true, Some(ranks)) = (linear, all_ranks) {
                    base *= f64::from(ranks);
                }
                linear && out.max_stacks.is_some()
            }
            None => stacking,
        };
        let mut changes: Vec<(AuraId, Change)> = Vec::new();
        for &source in &effect.modified_by {
            let Some(c) = change(input.dump, source, spell.id, effect.index) else {
                continue;
            };
            let holder = AuraId(source.0 .0);
            if (input.known_aura)(holder) {
                changes.push((holder, c));
            } else {
                out.notes.push(format!(
                    "#{} changed by {} #{}, which the data lacks",
                    effect.index, source.0 .0, source.1
                ));
            }
        }
        changes.dedup_by_key(|(a, _)| *a);
        let variants: Vec<(f64, Option<Predicate>)> =
            match changes.as_slice() {
                [] => vec![(base, None)],
                [(holder, c)] => vec![
                    (base, Some(Predicate::CasterLacksAura(*holder))),
                    (c.apply(base), Some(Predicate::CasterHasAura(*holder))),
                ],
                many => {
                    out.notes.push(format!(
                        "#{} changed by {} auras: their changes add as separate modifiers",
                        effect.index,
                        many.len()
                    ));
                    std::iter::once((base, None))
                        .chain(many.iter().map(|(h, c)| {
                            (c.apply(base) - base, Some(Predicate::CasterHasAura(*h)))
                        }))
                        .collect()
                }
            };
        for (amount, condition) in variants {
            if amount.abs() < 1e-9 {
                continue;
            }
            let modifier = |scope: ModScope| DerivedModifier {
                effect: effect.index,
                modifier: Modifier {
                    scope,
                    kind,
                    value: amount,
                    per_stack,
                    condition,
                },
            };
            match reach {
                Reach::Listed => {
                    for &target in &effect.affected_spells {
                        if (input.known_spell)(target) {
                            raw.push(modifier(ModScope::Spell(target)));
                        }
                    }
                }
                Reach::Schools => {
                    let mask = effect.misc_value.unwrap_or(ALL_SCHOOLS) & ALL_SCHOOLS;
                    let scope = if mask == ALL_SCHOOLS {
                        ModScope::All
                    } else {
                        ModScope::School(SchoolMask(mask as u8))
                    };
                    raw.push(modifier(scope));
                }
                Reach::AutoAttack => raw.push(modifier(ModScope::AutoAttack)),
                Reach::All => raw.push(modifier(ModScope::All)),
            }
        }
    }
    out.modifiers = merge_hit_kinds(raw);
    out
}

/// A direct- and a periodic-amount modifier alike in all else become one
/// `DamageDonePct`; a mastery's direct and periodic effects listing the
/// same spell become one.
fn merge_hit_kinds(raw: Vec<DerivedModifier>) -> Vec<DerivedModifier> {
    let same = |a: &Modifier, b: &Modifier| {
        a.scope == b.scope
            && (a.value - b.value).abs() < 1e-9
            && a.per_stack == b.per_stack
            && a.condition == b.condition
    };
    let mut out: Vec<DerivedModifier> = Vec::with_capacity(raw.len());
    let mut used = vec![false; raw.len()];
    for i in 0..raw.len() {
        if used[i] {
            continue;
        }
        let mut d = raw[i].clone();
        let partner = match d.modifier.kind {
            ModKind::DirectDamagePct => Some(ModKind::PeriodicDamagePct),
            ModKind::MasteryDamagePct => Some(ModKind::MasteryDamagePct),
            _ => None,
        };
        if let Some(partner) = partner {
            let found = (i + 1..raw.len()).find(|&j| {
                !used[j] && raw[j].modifier.kind == partner && same(&raw[j].modifier, &d.modifier)
            });
            if let Some(j) = found {
                used[j] = true;
                if d.modifier.kind == ModKind::DirectDamagePct {
                    d.modifier.kind = ModKind::DamageDonePct;
                }
            }
        }
        used[i] = true;
        out.push(d);
    }
    out
}

/// `data/derived_modifiers.ron`: the modifiers the dump gives a class's
/// auras, merged into `game.ron` on load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedFile {
    /// The `SpellDataDump` build read.
    pub build: GameBuild,
    pub auras: BTreeMap<AuraId, DerivedEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_stacks: Option<u8>,
    pub modifiers: Vec<Modifier>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// How many ranks of a talent grant `aura` at most; `None` when no talent
/// does.
fn rank_grants(data: &GameData, aura: AuraId) -> Option<u8> {
    data.talents
        .values()
        .map(|t| {
            t.ranks
                .iter()
                .filter(|grants| grants.contains(&Grant::PassiveAura(aura)))
                .count()
        })
        .max()
        .filter(|&n| n > 0)
        .map(|n| u8::try_from(n).unwrap_or(u8::MAX))
}

/// The derived modifiers of every aura of `classes` in `data` the dump
/// describes, and of those classes' spec passives.
pub fn derive_classes(data: &GameData, dump: &SpellDump, classes: &[&str]) -> DerivedFile {
    let of_class = |s: &DumpSpell| {
        s.class.as_deref().is_some_and(|c| {
            classes.iter().any(|class| {
                c == *class
                    || c.strip_suffix(class)
                        .is_some_and(|spec| spec.ends_with(' '))
            })
        })
    };
    let passives: Vec<AuraId> = data
        .specs
        .values()
        .filter(|s| classes.contains(&s.class.as_str()))
        .filter_map(|s| spec_passive_aura_id(s.id))
        .collect();
    let known_spell = |s: SpellId| data.spells.contains_key(&s);
    let known_aura = |a: AuraId| data.auras.contains_key(&a) || passives.contains(&a);
    let candidates = data.auras.keys().copied().chain(passives.iter().copied());
    let mut auras = BTreeMap::new();
    for id in candidates.filter(|id| !SPEC_SELECTED.contains(id)) {
        let Some(spell) = dump.spells.get(&SpellId(id.0)).filter(|s| of_class(s)) else {
            continue;
        };
        let input = DeriveInput {
            dump,
            known_spell: &known_spell,
            known_aura: &known_aura,
            rank_grants: rank_grants(data, id),
        };
        let derived = derive_aura(spell, &input);
        if derived.modifiers.is_empty() {
            continue;
        }
        auras.insert(
            id,
            DerivedEntry {
                name: spell.name.clone(),
                max_stacks: derived.max_stacks,
                modifiers: derived.modifiers.into_iter().map(|d| d.modifier).collect(),
                notes: derived.notes,
            },
        );
    }
    DerivedFile {
        build: dump.build.clone(),
        auras,
    }
}

/// Kinds a hit-kind split leaves comparable: a spell-wide bonus and its
/// direct or periodic part.
fn family(kind: ModKind) -> ModKind {
    match kind {
        ModKind::DirectDamagePct | ModKind::PeriodicDamagePct => ModKind::DamageDonePct,
        other => other,
    }
}

/// Whether the dump owns `m`, a hand-written modifier of an aura with
/// derived modifiers `derived`: a kind the aura's effects define, either
/// unconditional or conditioned on an aura whose change a derived variant
/// already carries.
pub fn derived_owns(m: &Modifier, derived: &[Modifier]) -> bool {
    if !DERIVED_KINDS.contains(&m.kind) {
        return false;
    }
    let same_family = |d: &&Modifier| family(d.kind) == family(m.kind);
    match m.condition {
        None => derived.iter().any(|d| same_family(&d)),
        Some(Predicate::CasterHasAura(a) | Predicate::CasterLacksAura(a)) => derived.iter().filter(same_family).any(
            |d| matches!(d.condition, Some(Predicate::CasterHasAura(b) | Predicate::CasterLacksAura(b)) if b == a),
        ),
        Some(_) => false,
    }
}

/// How an aura's hand-written modifiers compare with those its dump
/// effects give, over [`DERIVED_KINDS`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModifierDiff {
    /// Same scope, kind family, condition, and value.
    pub matched: usize,
    /// Paired; the hand-written modifier, then the derived value.
    pub value_differs: Vec<(Modifier, f64)>,
    /// Hand-written, with no derived counterpart.
    pub hand_only: Vec<Modifier>,
    /// Derived, with no hand-written counterpart.
    pub derived_only: Vec<DerivedModifier>,
}

impl ModifierDiff {
    pub fn is_clean(&self) -> bool {
        self.value_differs.is_empty() && self.hand_only.is_empty() && self.derived_only.is_empty()
    }
}

/// Compares `hand` with `derived`, pairing modifiers by scope, kind family,
/// and condition.
pub fn diff_modifiers(hand: &[Modifier], derived: &[DerivedModifier]) -> ModifierDiff {
    let mut diff = ModifierDiff::default();
    let mut unpaired: Vec<&DerivedModifier> = derived.iter().collect();
    for m in hand.iter().filter(|m| DERIVED_KINDS.contains(&m.kind)) {
        let pair = unpaired.iter().position(|d| {
            d.modifier.scope == m.scope
                && family(d.modifier.kind) == family(m.kind)
                && d.modifier.condition == m.condition
        });
        match pair {
            Some(i) => {
                let d = unpaired.remove(i);
                if (d.modifier.value - m.value).abs() < 1e-6 && d.modifier.kind == m.kind {
                    diff.matched += 1;
                } else {
                    diff.value_differs.push((*m, d.modifier.value));
                }
            }
            None => diff.hand_only.push(*m),
        }
    }
    diff.derived_only = unpaired.into_iter().cloned().collect();
    diff
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = "\
Name             : Lingering Darkness (id=457273) [Spell Family (8)] 
Class            : Rogue
Duration         : 30 seconds
Effects          :
#1 (id=1162160)  : Apply Aura (6) | Modify Damage Done% (79)
                   Base Value: 30 | Scaled Value: 30 | Misc Value: 0x8 | Target: Self (1)
                   Affected School(s): Nature
#2 (id=1162161)  : Apply Aura (6) | Add Percent Modifier (108): Spell Direct Amount (0)
                   Base Value: 10 | Scaled Value: 10 | Target: Self (1)
                   Affected Spells: Eviscerate (196819), Backstab (53)
                   Modified By: Deeper Daggers (382517 effect#1)
#3 (id=1162162)  : Apply Aura (6) | Add Percent Modifier (108): Spell Periodic Amount (22)
                   Base Value: 10 | Scaled Value: 10 | Target: Self (1)
                   Affected Spells: Eviscerate (196819), Rupture (1943)

Name             : Deeper Daggers (id=382517) [Spell Family (8), Passive] 
Class            : Rogue
Effects          :
#1 (id=1)        : Apply Aura (6) | Add Flat Modifier (107): Spell Effect 2 (12)
                   Base Value: 5 | Scaled Value: 5 | Target: Self (1)
                   Affected Spells: Lingering Darkness (457273)

Name             : Lethality (id=382238) [Spell Family (8), Passive] 
Talent Entry     : Subtlety [tree=spec, row=1, col=1, max_rank=2, req_points=0]
                 : Effect#1 [op=set, values=(1, 2)]
Class            : Rogue
Effects          :
#1 (id=2)        : Apply Aura (6) | Modify Critical Strike% (290)
                   Base Value: 1 | Scaled Value: 1 | Target: Self (1)
";

    fn input<'a>(
        dump: &'a SpellDump,
        known: &'a dyn Fn(SpellId) -> bool,
        grants: Option<u8>,
    ) -> DeriveInput<'a> {
        DeriveInput {
            dump,
            known_spell: known,
            known_aura: &|_| true,
            rank_grants: grants,
        }
    }

    #[test]
    fn effect_types_set_reach_and_changes_condition_variants() {
        let dump = SpellDump::parse_str(DUMP);
        let known = |s: SpellId| s != SpellId(53);
        let derived = derive_aura(
            &dump.spells[&SpellId(457273)],
            &input(&dump, &known, Some(1)),
        );
        let got: Vec<(ModScope, ModKind, f64, Option<Predicate>)> = derived
            .modifiers
            .iter()
            .map(|d| {
                (
                    d.modifier.scope,
                    d.modifier.kind,
                    d.modifier.value,
                    d.modifier.condition,
                )
            })
            .collect();
        let daggers = AuraId(382517);
        assert_eq!(
            got,
            vec![
                (
                    ModScope::School(SchoolMask::NATURE),
                    ModKind::DamageDonePct,
                    30.0,
                    None
                ),
                (
                    ModScope::Spell(SpellId(196819)),
                    ModKind::DirectDamagePct,
                    10.0,
                    Some(Predicate::CasterLacksAura(daggers))
                ),
                (
                    ModScope::Spell(SpellId(196819)),
                    ModKind::DirectDamagePct,
                    15.0,
                    Some(Predicate::CasterHasAura(daggers))
                ),
                (
                    ModScope::Spell(SpellId(196819)),
                    ModKind::PeriodicDamagePct,
                    10.0,
                    None
                ),
                (
                    ModScope::Spell(SpellId(1943)),
                    ModKind::PeriodicDamagePct,
                    10.0,
                    None
                ),
            ]
        );
    }

    #[test]
    fn linear_ranks_stack() {
        let dump = SpellDump::parse_str(DUMP);
        let lethality = &dump.spells[&SpellId(382238)];
        let derived = derive_aura(lethality, &input(&dump, &|_| true, Some(2)));
        assert_eq!(derived.max_stacks, Some(2));
        assert!(derived.modifiers[0].modifier.per_stack);
        let once = derive_aura(lethality, &input(&dump, &|_| true, Some(1)));
        assert_eq!(once.max_stacks, None);
        assert!(!once.modifiers[0].modifier.per_stack);
        let baseline = derive_aura(lethality, &input(&dump, &|_| true, None));
        assert_eq!(
            (baseline.max_stacks, baseline.modifiers[0].modifier.value),
            (None, 2.0)
        );
    }

    #[test]
    fn direct_and_periodic_alike_merge() {
        let dump = SpellDump::parse_str(
            &DUMP.replace("Modified By: Deeper Daggers (382517 effect#1)", ""),
        );
        let derived = derive_aura(
            &dump.spells[&SpellId(457273)],
            &input(&dump, &|_| true, Some(1)),
        );
        let eviscerate: Vec<ModKind> = derived
            .modifiers
            .iter()
            .filter(|d| d.modifier.scope == ModScope::Spell(SpellId(196819)))
            .map(|d| d.modifier.kind)
            .collect();
        assert_eq!(eviscerate, vec![ModKind::DamageDonePct]);
    }

    #[test]
    fn dump_owns_kinds_its_effects_define() {
        let dump = SpellDump::parse_str(DUMP);
        let derived: Vec<Modifier> = derive_aura(
            &dump.spells[&SpellId(457273)],
            &input(&dump, &|_| true, Some(1)),
        )
        .modifiers
        .into_iter()
        .map(|d| d.modifier)
        .collect();
        let hand = |kind, condition| Modifier {
            scope: ModScope::All,
            kind,
            value: 5.0,
            per_stack: false,
            condition,
        };
        assert!(derived_owns(&hand(ModKind::DamageDonePct, None), &derived));
        assert!(!derived_owns(&hand(ModKind::CritDamagePct, None), &derived));
        assert!(derived_owns(
            &hand(
                ModKind::DamageDonePct,
                Some(Predicate::CasterHasAura(AuraId(382517)))
            ),
            &derived
        ));
        assert!(!derived_owns(
            &hand(
                ModKind::DamageDonePct,
                Some(Predicate::CasterHasAura(AuraId(1)))
            ),
            &derived
        ));
    }
}
