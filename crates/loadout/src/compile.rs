use std::collections::{BTreeMap, BTreeSet};

use portunus_core::{AuraId, ItemSetId, SpellId, TalentId};
use portunus_gamedata::item::{EquipKind, GearSlot, ItemDef};
use portunus_gamedata::spec::SpecDef;
use portunus_gamedata::stats::{DerivedStats, RatingCurve, Stat, StatBlock, StatCurves};
use portunus_gamedata::talent::{Grant, HeroTreeDef, TalentNode, TalentTree};
use portunus_gamedata::GameData;

use crate::{ActorTemplate, Loadout, LoadoutCompiler, LoadoutError, LoadoutIssue};

/// The loadout compiler.
#[derive(Debug, Clone, Copy, Default)]
pub struct Compiler;

impl LoadoutCompiler for Compiler {
    fn validate(&self, data: &GameData, loadout: &Loadout) -> Vec<LoadoutIssue> {
        let mut issues = Vec::new();
        let Some(spec) = data.specs.get(&loadout.spec) else {
            return vec![LoadoutIssue::UnknownSpec(loadout.spec)];
        };
        check_gear(data, loadout, &mut issues);
        check_talents(data, spec, loadout, &mut issues);
        check_on_use(data, loadout, &mut issues);
        issues
    }

    fn compile(&self, data: &GameData, loadout: &Loadout) -> Result<ActorTemplate, LoadoutError> {
        let issues = self.validate(data, loadout);
        if !issues.is_empty() {
            return Err(LoadoutError::Invalid(issues));
        }
        let spec = &data.specs[&loadout.spec];
        let stats = total_stats(data, spec, loadout);
        let derived = derive(&data.curves, spec, &stats);
        let (abilities, passive_auras) = grants(data, spec, loadout);
        let weapon = |slot| {
            loadout
                .gear
                .get(&slot)
                .and_then(|e| data.items[&e.item].weapon)
        };
        Ok(ActorTemplate {
            spec: spec.id,
            role: spec.role,
            hero_tree: loadout.hero_tree,
            stats,
            derived,
            abilities,
            passive_auras,
            resources: spec.resources.clone(),
            main_hand: weapon(GearSlot::MainHand),
            off_hand: weapon(GearSlot::OffHand),
            permanent_pet: None,
            source: loadout.clone(),
        })
    }
}

fn fits(slot: GearSlot, kind: EquipKind) -> bool {
    use EquipKind as K;
    use GearSlot as S;
    match slot {
        S::Head => kind == K::Head,
        S::Neck => kind == K::Neck,
        S::Shoulder => kind == K::Shoulder,
        S::Back => kind == K::Back,
        S::Chest => kind == K::Chest,
        S::Wrist => kind == K::Wrist,
        S::Hands => kind == K::Hands,
        S::Waist => kind == K::Waist,
        S::Legs => kind == K::Legs,
        S::Feet => kind == K::Feet,
        S::Finger1 | S::Finger2 => kind == K::Finger,
        S::Trinket1 | S::Trinket2 => kind == K::Trinket,
        S::MainHand => matches!(kind, K::OneHand | K::TwoHand | K::MainHandOnly),
        S::OffHand => matches!(kind, K::OneHand | K::OffHand),
    }
}

fn check_gear(data: &GameData, loadout: &Loadout, issues: &mut Vec<LoadoutIssue>) {
    let mut unique: BTreeMap<u32, portunus_core::ItemId> = BTreeMap::new();
    for (&slot, equipped) in &loadout.gear {
        if !data.curves.item_budget.contains_key(&equipped.item_level) {
            issues.push(LoadoutIssue::UnknownItemLevel(equipped.item_level));
        }
        if let Some(aura) = equipped.enchant {
            if !data.auras.contains_key(&aura) {
                issues.push(LoadoutIssue::UnknownAura(aura));
            }
        }
        for gem in &equipped.gems {
            match data.items.get(gem) {
                None => issues.push(LoadoutIssue::UnknownItem(*gem)),
                Some(def) if def.equip != EquipKind::Gem => {
                    issues.push(LoadoutIssue::NotAGem(*gem));
                }
                Some(_) => {}
            }
        }
        let Some(def) = data.items.get(&equipped.item) else {
            issues.push(LoadoutIssue::UnknownItem(equipped.item));
            continue;
        };
        if !fits(slot, def.equip) {
            issues.push(LoadoutIssue::WrongSlot { slot, item: def.id });
        }
        if let Some(group) = def.unique_group {
            if let Some(&other) = unique.get(&group) {
                issues.push(LoadoutIssue::UniqueConflict {
                    a: other,
                    b: def.id,
                });
            } else {
                unique.insert(group, def.id);
            }
        }
    }
    for item in &loadout.consumables {
        match data.items.get(item) {
            None => issues.push(LoadoutIssue::UnknownItem(*item)),
            Some(def) if def.equip != EquipKind::Consumable => {
                issues.push(LoadoutIssue::NotConsumable(*item));
            }
            Some(_) => {}
        }
    }
}

/// The loadout's hero tree, if it exists and the spec may choose it.
fn hero_tree<'a>(data: &'a GameData, spec: &SpecDef, loadout: &Loadout) -> Option<&'a HeroTreeDef> {
    let tree = data.hero_trees.get(&loadout.hero_tree?)?;
    tree.specs.contains(&spec.id).then_some(tree)
}

/// Talents taken at rank 1 or more, keyed by id. `spent` leaves out the
/// free hero keystone; `taken` includes it.
struct Taken {
    taken: BTreeMap<TalentId, u8>,
    spent: BTreeMap<TalentId, u8>,
}

fn taken(data: &GameData, spec: &SpecDef, loadout: &Loadout) -> Taken {
    let mut spent: BTreeMap<TalentId, u8> = loadout
        .talents
        .0
        .iter()
        .filter(|(_, &rank)| rank > 0)
        .map(|(&t, &r)| (t, r))
        .collect();
    let mut taken = spent.clone();
    if let Some(hero) = hero_tree(data, spec, loadout) {
        let rank = taken.entry(hero.keystone).or_insert(1);
        if let Some(paid) = spent.get_mut(&hero.keystone) {
            *paid = rank.saturating_sub(1);
        }
    }
    Taken { taken, spent }
}

/// The tree (by index into `trees`), node index, and node holding a talent.
fn find_node<'a>(
    trees: &[&'a TalentTree],
    talent: TalentId,
) -> Option<(usize, usize, &'a TalentNode)> {
    trees.iter().enumerate().find_map(|(t, tree)| {
        tree.nodes
            .iter()
            .enumerate()
            .find(|(_, n)| n.choices.contains(&talent))
            .map(|(i, n)| (t, i, n))
    })
}

fn check_talents(
    data: &GameData,
    spec: &SpecDef,
    loadout: &Loadout,
    issues: &mut Vec<LoadoutIssue>,
) {
    if let Some(id) = loadout.hero_tree {
        match data.hero_trees.get(&id) {
            None => issues.push(LoadoutIssue::UnknownHeroTree(id)),
            Some(h) if !h.specs.contains(&spec.id) => {
                issues.push(LoadoutIssue::HeroTreeNotForSpec(id));
            }
            Some(_) => {}
        }
    }
    let hero = hero_tree(data, spec, loadout);
    let trees: Vec<&TalentTree> = spec.trees.iter().chain(hero.map(|h| &h.tree)).collect();
    let Taken { taken, spent } = taken(data, spec, loadout);
    let mut spent_by_tree: BTreeMap<usize, u8> = BTreeMap::new();
    let mut node_picks: BTreeMap<(usize, usize), TalentId> = BTreeMap::new();

    for (&talent, &rank) in &taken {
        if !data.talents.contains_key(&talent) {
            issues.push(LoadoutIssue::UnknownTalent(talent));
            continue;
        }
        let Some((tree_index, node_index, node)) = find_node(&trees, talent) else {
            let elsewhere = data
                .hero_trees
                .values()
                .any(|h| h.tree.nodes.iter().any(|n| n.choices.contains(&talent)));
            issues.push(if elsewhere {
                LoadoutIssue::HeroTalentWithoutTree(talent)
            } else {
                LoadoutIssue::TalentUnreachable(talent)
            });
            continue;
        };
        let tree = trees[tree_index];
        if rank > node.max_rank {
            issues.push(LoadoutIssue::TalentRankTooHigh { talent, rank });
        }
        if let Some(&other) = node_picks.get(&(tree_index, node_index)) {
            issues.push(LoadoutIssue::ChoiceConflict {
                a: other,
                b: talent,
            });
        } else {
            node_picks.insert((tree_index, node_index), talent);
        }
        let total = spent_by_tree.entry(tree_index).or_default();
        *total = total.saturating_add(spent.get(&talent).copied().unwrap_or(0));

        let prerequisite_met =
            node.requires_any.is_empty() || node.requires_any.iter().any(|t| taken.contains_key(t));
        let gates_met = tree.gates.iter().all(|&(gate_row, need)| {
            node.row < gate_row || points_above(tree, &spent, gate_row) >= need
        });
        if !prerequisite_met || !gates_met {
            issues.push(LoadoutIssue::TalentUnreachable(talent));
        }
    }

    for (i, tree) in trees.iter().enumerate() {
        let spent = spent_by_tree.get(&i).copied().unwrap_or(0);
        if spent > tree.points {
            issues.push(LoadoutIssue::TooManyTalentPoints {
                tree: tree.name.clone(),
                spent,
                allowed: tree.points,
            });
        }
    }
}

fn points_above(tree: &TalentTree, spent: &BTreeMap<TalentId, u8>, row: u8) -> u8 {
    tree.nodes
        .iter()
        .filter(|n| n.row < row)
        .flat_map(|n| &n.choices)
        .filter_map(|t| spent.get(t))
        .fold(0u8, |sum, &r| sum.saturating_add(r))
}

fn on_use_items<'a>(data: &'a GameData, loadout: &'a Loadout) -> impl Iterator<Item = &'a ItemDef> {
    loadout
        .gear
        .values()
        .map(|e| e.item)
        .chain(loadout.consumables.iter().copied())
        .filter_map(|id| data.items.get(&id))
}

fn check_on_use(data: &GameData, loadout: &Loadout, issues: &mut Vec<LoadoutIssue>) {
    let mut seen = BTreeSet::new();
    for spell in on_use_items(data, loadout).filter_map(|d| d.on_use) {
        if !seen.insert(spell) {
            issues.push(LoadoutIssue::DuplicateOnUse(spell));
        }
    }
}

fn total_stats(data: &GameData, spec: &SpecDef, loadout: &Loadout) -> StatBlock {
    let mut stats = spec.base_stats.clone();
    for equipped in loadout.gear.values() {
        let def = &data.items[&equipped.item];
        let budget = data.curves.item_budget[&equipped.item_level];
        for &(stat, share) in &def.stat_allocation {
            *stats.0.entry(stat).or_default() += budget * share;
        }
    }
    stats
}

/// Ratings to percent, each band past a threshold keeping its own fraction.
fn rating_pct(curve: &RatingCurve, rating: f64) -> f64 {
    let raw = rating / curve.rating_per_pct;
    let mut out = 0.0;
    let mut prev = 0.0;
    let mut keep = 1.0;
    for &(threshold, fraction) in &curve.diminishing {
        if raw <= threshold {
            break;
        }
        out += (threshold - prev) * keep;
        prev = threshold;
        keep = fraction;
    }
    out + (raw - prev) * keep
}

/// Percentages and derived values for a stat block. Mechanics call this
/// again whenever auras change a seat's stats.
pub fn derive(curves: &StatCurves, spec: &SpecDef, stats: &StatBlock) -> DerivedStats {
    let stat = |s: Stat| stats.0.get(&s).copied().unwrap_or(0.0);
    let pct = |s: Stat| {
        curves
            .ratings
            .get(&s)
            .map_or(0.0, |curve| rating_pct(curve, stat(s)))
    };
    let primary_for_ap = match spec.primary_stat {
        Stat::Agility | Stat::Strength => stat(spec.primary_stat),
        _ => 0.0,
    };
    DerivedStats {
        crit_pct: curves.base_crit_pct + pct(Stat::CritRating),
        haste_pct: pct(Stat::HasteRating),
        mastery_pct: spec.base_mastery_pct + pct(Stat::MasteryRating) * spec.mastery_coef,
        versatility_pct: pct(Stat::VersatilityRating),
        attack_power: primary_for_ap * curves.attack_power_per_primary,
        spell_power: stat(Stat::Intellect) * curves.spell_power_per_intellect,
        max_health: stat(Stat::Stamina) * curves.health_per_stamina,
    }
}

/// Abilities and permanent auras, in a deterministic order with duplicates
/// dropped.
fn grants(data: &GameData, spec: &SpecDef, loadout: &Loadout) -> (BTreeSet<SpellId>, Vec<AuraId>) {
    let mut abilities: BTreeSet<SpellId> = spec.baseline_spells.iter().copied().collect();
    let mut auras: Vec<AuraId> = spec.baseline_auras.clone();
    let mut replacements = Vec::new();

    for (talent, &rank) in &taken(data, spec, loadout).taken {
        let def = &data.talents[talent];
        for grant in def.ranks.iter().take(usize::from(rank)).flatten() {
            match *grant {
                Grant::Spell(spell) => {
                    abilities.insert(spell);
                }
                Grant::PassiveAura(aura) => auras.push(aura),
                Grant::ReplaceSpell { from, to } => replacements.push((from, to)),
            }
        }
    }
    for (from, to) in replacements {
        if abilities.remove(&from) {
            abilities.insert(to);
        }
    }

    let mut set_pieces: BTreeMap<ItemSetId, u8> = BTreeMap::new();
    for equipped in loadout.gear.values() {
        let def = &data.items[&equipped.item];
        auras.extend(&def.passive_auras);
        auras.extend(equipped.enchant);
        for gem in &equipped.gems {
            auras.extend(&data.items[gem].passive_auras);
        }
        if let Some(set) = def.set {
            *set_pieces.entry(set).or_default() += 1;
        }
    }
    for item in &loadout.consumables {
        auras.extend(&data.items[item].passive_auras);
    }
    for (set, pieces) in set_pieces {
        if let Some(def) = data.item_sets.get(&set) {
            auras.extend(
                def.bonuses
                    .iter()
                    .filter(|(need, _)| *need <= pieces)
                    .map(|(_, aura)| *aura),
            );
        }
    }
    abilities.extend(on_use_items(data, loadout).filter_map(|d| d.on_use));

    let mut seen = BTreeSet::new();
    auras.retain(|a| seen.insert(*a));
    (abilities, auras)
}

#[cfg(test)]
mod tests {
    use super::*;
    use portunus_core::{HeroTreeId, ItemId, SpecId};
    use portunus_gamedata::item::ItemSetDef;
    use portunus_gamedata::spec::Role;
    use portunus_gamedata::talent::TalentDef;
    use portunus_gamedata::GameBuild;

    use crate::{EquippedItem, TalentSelection};

    const SPEC: SpecId = SpecId(1);

    fn item(id: u32, equip: EquipKind) -> ItemDef {
        ItemDef {
            id: ItemId(id),
            name: format!("item {id}"),
            equip,
            stat_allocation: Vec::new(),
            weapon: None,
            passive_auras: Vec::new(),
            on_use: None,
            set: None,
            unique_group: None,
        }
    }

    fn talent(id: u32, ranks: Vec<Vec<Grant>>) -> (TalentId, TalentDef) {
        (
            TalentId(id),
            TalentDef {
                id: TalentId(id),
                name: format!("talent {id}"),
                ranks,
            },
        )
    }

    /// A one-point hero tree: the keystone, then `rest` below it.
    fn hero(id: u32, spec: SpecId, keystone: u32, rest: &[u32]) -> HeroTreeDef {
        let below = rest.iter().map(|&t| TalentNode {
            row: 1,
            choices: vec![TalentId(t)],
            max_rank: 1,
            requires_any: vec![TalentId(keystone)],
        });
        HeroTreeDef {
            id: HeroTreeId(id),
            name: format!("hero {id}"),
            specs: vec![spec],
            tree: TalentTree {
                name: format!("hero {id}"),
                points: 1,
                nodes: std::iter::once(TalentNode {
                    row: 0,
                    choices: vec![TalentId(keystone)],
                    max_rank: 1,
                    requires_any: Vec::new(),
                })
                .chain(below)
                .collect(),
                gates: Vec::new(),
            },
            keystone: TalentId(keystone),
        }
    }

    fn data() -> GameData {
        let mut helm = item(10, EquipKind::Head);
        helm.stat_allocation = vec![(Stat::Intellect, 0.5), (Stat::CritRating, 0.25)];
        helm.set = Some(ItemSetId(1));
        let mut ring = item(11, EquipKind::Finger);
        ring.unique_group = Some(7);
        let mut trinket = item(12, EquipKind::Trinket);
        trinket.on_use = Some(SpellId(900));
        let mut potion = item(13, EquipKind::Consumable);
        potion.on_use = Some(SpellId(901));
        let mut flask = item(14, EquipKind::Consumable);
        flask.passive_auras = vec![AuraId(500)];

        let tree = TalentTree {
            name: "spec".into(),
            points: 3,
            nodes: vec![
                TalentNode {
                    row: 0,
                    choices: vec![TalentId(1)],
                    max_rank: 2,
                    requires_any: Vec::new(),
                },
                TalentNode {
                    row: 1,
                    choices: vec![TalentId(2), TalentId(3)],
                    max_rank: 1,
                    requires_any: vec![TalentId(1)],
                },
                TalentNode {
                    row: 2,
                    choices: vec![TalentId(4)],
                    max_rank: 1,
                    requires_any: Vec::new(),
                },
            ],
            gates: vec![(2, 2)],
        };
        let spec = SpecDef {
            id: SPEC,
            class: "Test".into(),
            name: "Test".into(),
            role: Role::Damage,
            primary_stat: Stat::Intellect,
            base_stats: StatBlock(BTreeMap::from([(Stat::Intellect, 100.0)])),
            base_mastery_pct: 8.0,
            mastery_coef: 2.0,
            resources: Vec::new(),
            baseline_spells: vec![SpellId(100), SpellId(101)],
            baseline_auras: vec![AuraId(400)],
            trees: vec![tree],
        };
        GameData {
            build: GameBuild {
                version: "test".into(),
                build: 0,
            },
            specs: BTreeMap::from([(SPEC, spec)]),
            spells: BTreeMap::new(),
            auras: BTreeMap::new(),
            items: [helm, ring, trinket, potion, flask]
                .into_iter()
                .map(|i| (i.id, i))
                .collect(),
            item_sets: BTreeMap::from([(
                ItemSetId(1),
                ItemSetDef {
                    id: ItemSetId(1),
                    name: "set".into(),
                    bonuses: vec![(1, AuraId(600)), (2, AuraId(601))],
                },
            )]),
            talents: [
                talent(
                    1,
                    vec![
                        vec![Grant::PassiveAura(AuraId(410))],
                        vec![Grant::Spell(SpellId(102))],
                    ],
                ),
                talent(
                    2,
                    vec![vec![Grant::ReplaceSpell {
                        from: SpellId(101),
                        to: SpellId(103),
                    }]],
                ),
                talent(3, vec![vec![]]),
                talent(4, vec![vec![]]),
                talent(20, vec![vec![Grant::PassiveAura(AuraId(420))]]),
                talent(
                    21,
                    vec![vec![Grant::ReplaceSpell {
                        from: SpellId(100),
                        to: SpellId(105),
                    }]],
                ),
                talent(22, vec![vec![]]),
                talent(30, vec![vec![]]),
                talent(31, vec![vec![]]),
                talent(40, vec![vec![]]),
            ]
            .into_iter()
            .collect(),
            hero_trees: [
                hero(1, SPEC, 20, &[21, 22]),
                hero(2, SPEC, 30, &[31]),
                hero(3, SpecId(2), 40, &[]),
            ]
            .into_iter()
            .map(|h| (h.id, h))
            .collect(),
            pets: BTreeMap::new(),
            curves: StatCurves {
                ratings: BTreeMap::from([(
                    Stat::CritRating,
                    RatingCurve {
                        rating_per_pct: 10.0,
                        diminishing: vec![(30.0, 0.5)],
                    },
                )]),
                item_budget: BTreeMap::from([(400, 1000.0)]),
                base_crit_pct: 5.0,
                spell_power_per_intellect: 1.0,
                attack_power_per_primary: 1.0,
                health_per_stamina: 20.0,
                armor_constant: 7390.0,
            },
        }
    }

    fn equipped(item: u32) -> EquippedItem {
        EquippedItem {
            item: ItemId(item),
            item_level: 400,
            enchant: None,
            gems: Vec::new(),
        }
    }

    fn loadout() -> Loadout {
        Loadout {
            spec: SPEC,
            gear: BTreeMap::from([
                (GearSlot::Head, equipped(10)),
                (GearSlot::Trinket1, equipped(12)),
            ]),
            hero_tree: None,
            talents: TalentSelection(BTreeMap::from([(TalentId(1), 2), (TalentId(2), 1)])),
            consumables: vec![ItemId(13), ItemId(14)],
        }
    }

    #[test]
    fn compiles_stats_abilities_and_auras() {
        let t = Compiler.compile(&data(), &loadout()).unwrap();
        assert_eq!(t.stats.0[&Stat::Intellect], 600.0);
        assert_eq!(t.stats.0[&Stat::CritRating], 250.0);
        assert_eq!(t.derived.spell_power, 600.0);
        assert_eq!(t.derived.crit_pct, 5.0 + 25.0);
        assert_eq!(t.derived.mastery_pct, 8.0);
        assert_eq!(
            t.abilities.into_iter().collect::<Vec<_>>(),
            [100, 102, 103, 900, 901].map(SpellId)
        );
        assert_eq!(t.passive_auras, [400, 410, 500, 600].map(AuraId));
    }

    #[test]
    fn diminishing_returns_apply_per_band() {
        let curve = RatingCurve {
            rating_per_pct: 10.0,
            diminishing: vec![(30.0, 0.5), (40.0, 0.25)],
        };
        assert_eq!(rating_pct(&curve, 200.0), 20.0);
        assert_eq!(rating_pct(&curve, 400.0), 30.0 + 5.0);
        assert_eq!(rating_pct(&curve, 600.0), 30.0 + 5.0 + 5.0);
    }

    #[test]
    fn reports_every_problem() {
        let mut bad = loadout();
        bad.gear.insert(GearSlot::Neck, equipped(10));
        bad.gear.insert(GearSlot::Finger1, equipped(11));
        bad.gear.insert(GearSlot::Finger2, equipped(11));
        bad.gear.insert(GearSlot::Trinket2, equipped(12));
        bad.gear.get_mut(&GearSlot::Head).unwrap().item_level = 999;
        bad.consumables.push(ItemId(10));
        bad.talents = TalentSelection(BTreeMap::from([
            (TalentId(1), 3),
            (TalentId(2), 1),
            (TalentId(3), 1),
            (TalentId(4), 1),
            (TalentId(99), 1),
        ]));
        let issues = Compiler.validate(&data(), &bad);
        for expected in [
            LoadoutIssue::WrongSlot {
                slot: GearSlot::Neck,
                item: ItemId(10),
            },
            LoadoutIssue::UniqueConflict {
                a: ItemId(11),
                b: ItemId(11),
            },
            LoadoutIssue::DuplicateOnUse(SpellId(900)),
            LoadoutIssue::UnknownItemLevel(999),
            LoadoutIssue::NotConsumable(ItemId(10)),
            LoadoutIssue::TalentRankTooHigh {
                talent: TalentId(1),
                rank: 3,
            },
            LoadoutIssue::ChoiceConflict {
                a: TalentId(2),
                b: TalentId(3),
            },
            LoadoutIssue::UnknownTalent(TalentId(99)),
            LoadoutIssue::TooManyTalentPoints {
                tree: "spec".into(),
                spent: 6,
                allowed: 3,
            },
        ] {
            assert!(
                issues.contains(&expected),
                "missing {expected:?} in {issues:?}"
            );
        }
    }

    #[test]
    fn prerequisites_and_gates() {
        let mut l = loadout();
        l.talents = TalentSelection(BTreeMap::from([(TalentId(2), 1), (TalentId(4), 1)]));
        let issues = Compiler.validate(&data(), &l);
        assert!(issues.contains(&LoadoutIssue::TalentUnreachable(TalentId(2))));
        assert!(issues.contains(&LoadoutIssue::TalentUnreachable(TalentId(4))));

        l.talents = TalentSelection(BTreeMap::from([(TalentId(1), 2), (TalentId(4), 1)]));
        assert!(Compiler.validate(&data(), &l).is_empty());
    }

    #[test]
    fn hero_trees_grant_their_keystone_for_free() {
        let mut l = loadout();
        l.hero_tree = Some(HeroTreeId(1));
        l.talents.0.insert(TalentId(21), 1);
        let t = Compiler.compile(&data(), &l).unwrap();
        assert_eq!(t.hero_tree, Some(HeroTreeId(1)));
        assert!(t.passive_auras.contains(&AuraId(420)), "the keystone");
        assert!(t.abilities.contains(&SpellId(105)));
        assert!(
            !t.abilities.contains(&SpellId(100)),
            "replaced by a hero talent"
        );

        // Naming the keystone changes nothing and costs nothing.
        l.talents.0.insert(TalentId(20), 1);
        let named = Compiler.compile(&data(), &l).unwrap();
        assert_eq!(named.abilities, t.abilities);
        assert_eq!(named.passive_auras, t.passive_auras);
    }

    #[test]
    fn hero_talents_need_their_tree() {
        let check = |hero: Option<u32>, extra: &[u32]| {
            let mut l = loadout();
            l.hero_tree = hero.map(HeroTreeId);
            l.talents.0.extend(extra.iter().map(|&t| (TalentId(t), 1)));
            Compiler.validate(&data(), &l)
        };
        assert_eq!(check(Some(2), &[31]), Vec::new());
        assert_eq!(
            check(None, &[21]),
            [LoadoutIssue::HeroTalentWithoutTree(TalentId(21))]
        );
        assert_eq!(
            check(Some(2), &[21]),
            [LoadoutIssue::HeroTalentWithoutTree(TalentId(21))],
            "only one hero tree at a time"
        );
        assert_eq!(
            check(Some(3), &[]),
            [LoadoutIssue::HeroTreeNotForSpec(HeroTreeId(3))]
        );
        assert_eq!(
            check(Some(9), &[]),
            [LoadoutIssue::UnknownHeroTree(HeroTreeId(9))]
        );
        assert_eq!(
            check(Some(1), &[21, 22]),
            [LoadoutIssue::TooManyTalentPoints {
                tree: "hero 1".into(),
                spent: 2,
                allowed: 1,
            }]
        );
    }
}
