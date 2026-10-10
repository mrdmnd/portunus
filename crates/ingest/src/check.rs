//! Reference checks for loaded tables: every id points at something, and
//! every table entry sits under its own id.

use std::collections::{BTreeMap, BTreeSet};

use portunus_core::{
    AuraId, EnemyKey, HeroTreeId, ItemId, ItemSetId, PetId, Sample, SimDuration, SpecId, SpellId,
    TalentId,
};
use portunus_gamedata::aura::{AuraValueKind, BankDraw};
use portunus_gamedata::effect::{
    Coefficient, Effect, EffectTarget, ListenFor, Listener, ModScope, Predicate, ProcChance,
    TargetCount, RECENT_CASTS,
};
use portunus_gamedata::enemy::{EnemyAction, EnemyKind};
use portunus_gamedata::spell::{CastKind, CooldownDef, Requirement};
use portunus_gamedata::stats::{ResourceDef, Stat};
use portunus_gamedata::talent::Grant;
use portunus_gamedata::{EnemyData, GameData};

/// Where a reference was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Spec(SpecId),
    Class(String),
    Spell(SpellId),
    Aura(AuraId),
    Item(ItemId),
    ItemSet(ItemSetId),
    Talent(TalentId),
    HeroTree(HeroTreeId),
    Pet(PetId),
    Enemy(EnemyKey),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataIssue {
    /// An entry's own id differs from the key it is stored under.
    KeyMismatch(Owner),
    UnknownSpell {
        owner: Owner,
        spell: SpellId,
    },
    UnknownAura {
        owner: Owner,
        aura: AuraId,
    },
    UnknownPet {
        owner: Owner,
        pet: PetId,
    },
    UnknownTalent {
        owner: Owner,
        talent: TalentId,
    },
    UnknownSpec {
        owner: Owner,
        spec: SpecId,
    },
    /// A hero tree's keystone is not one of its own nodes.
    KeystoneNotInTree(HeroTreeId),
    /// A talent sits in more than one of a spec's trees (counting the hero
    /// trees it may choose), so a loadout can't say which it took.
    TalentInTwoTrees {
        spec: SpecId,
        talent: TalentId,
    },
    UnknownItemSet {
        owner: Owner,
        set: ItemSetId,
    },
    UnknownEnemy {
        owner: Owner,
        enemy: EnemyKey,
    },
    /// A `per_count` percentage that isn't finite.
    InvalidCountScale(Owner),
    /// `SpawnAdds` names an enemy that isn't an `Add`, or spawns none.
    InvalidAdds {
        owner: Owner,
        enemy: EnemyKey,
    },
    /// Non-positive rating per percent, or thresholds out of order or
    /// fractions outside `[0, 1]`.
    InvalidRatingCurve(Stat),
    /// An enemy has zero health.
    InvalidHealth(EnemyKey),
    /// A deck-of-cards proc with no cards, or more successes than cards.
    InvalidDeck(Owner),
    /// A per-unit listener that doesn't listen for a resource spend.
    PerUnitWithoutSpend(Owner),
    /// An AoE rule's secondary share is outside `[0, 1]`.
    InvalidAoeShare(Owner),
    /// A listener sharing proc bookkeeping with one that isn't earlier on
    /// its aura, or that rolls a different chance.
    InvalidSharedProc(Owner),
    /// An `ApplyOneOf` with no auras to choose from.
    EmptyChoice(Owner),
    /// A `Chance` outside `[0, 1]`.
    InvalidChance(Owner),
    /// A missile speed that isn't a positive number.
    InvalidSpeed(SpellId),
    /// A range that isn't a positive number.
    InvalidRange(SpellId),
    /// A channel with no ticks or no duration.
    InvalidChannel(SpellId),
    /// An empower whose stage times aren't positive and strictly
    /// increasing, or whose stage effects don't match its stages.
    InvalidEmpower(SpellId),
    /// Spells sharing a cooldown category that disagree on its duration,
    /// charges, or haste.
    CategoryMismatch(u32),
    /// A valued aura whose initial value, cap, or threshold doesn't scale
    /// by a positive number, or a bank with no periodic ticks to draw it
    /// or a draw fraction outside `(0, 1]`.
    InvalidAuraValue(AuraId),
    /// A distance, displacement, or movement demand that isn't a positive
    /// number, or a demand with no time to meet it.
    InvalidMovement(Owner),
    /// A dual-wield miss chance outside `[0, 100]` percent.
    InvalidMissChance,
    /// A condition that can't mean anything: a health fraction outside
    /// `[0, 1]`, a stack count of zero, a value that isn't a number, or a
    /// recent-cast count outside `1..=RECENT_CASTS`.
    InvalidPredicate(Owner),
    /// A spell requirement with a health fraction outside `[0, 1]` or a
    /// stack count of zero.
    InvalidRequirement(SpellId),
    /// A recharging resource with a linear regen too, no units refilling
    /// at once, no period, or a maximum or start that isn't a whole
    /// number of units.
    InvalidRecharge(Owner),
    /// An aura applied for longer per unit spent with no time per unit, or
    /// with no duration to add it to.
    InvalidSpentDuration(Owner),
    /// An out-of-combat rate that isn't finite, or on a resource that
    /// recharges a unit at a time.
    InvalidOutOfCombat(Owner),
    /// A listener that can never fire: a health line outside `1..=100`
    /// percent, or a stack count of zero or above the aura's maximum.
    InvalidListener(Owner),
    /// A death prevention that heals to a share of health outside `(0, 1]`.
    InvalidDeathPrevention(AuraId),
}

fn positive(x: f64) -> bool {
    x > 0.0 && x.is_finite()
}

/// The number a coefficient scales by.
fn factor(c: Coefficient) -> f64 {
    match c {
        Coefficient::Flat(x)
        | Coefficient::AttackPower(x)
        | Coefficient::SpellPower(x)
        | Coefficient::WeaponDamage(x)
        | Coefficient::PctMaxHealth(x)
        | Coefficient::EventAmount(x)
        | Coefficient::AuraValue(x)
        | Coefficient::WeaponSpeed(x) => x,
    }
}

pub fn check_game_data(data: &GameData) -> Vec<DataIssue> {
    let mut c = Checker {
        data,
        issues: Vec::new(),
    };
    c.keys();
    for spec in data.specs.values() {
        let owner = Owner::Spec(spec.id);
        spec.resources.iter().for_each(|r| c.resource(&owner, r));
        spec.baseline_spells
            .iter()
            .for_each(|&s| c.spell(&owner, s));
        spec.baseline_auras.iter().for_each(|&a| c.aura(&owner, a));
        for talent in spec
            .trees
            .iter()
            .flat_map(|t| &t.nodes)
            .flat_map(|n| n.choices.iter().chain(&n.requires_any))
        {
            c.talent(&owner, *talent);
        }
        let hero = data
            .hero_trees
            .values()
            .filter(|h| h.specs.contains(&spec.id))
            .map(|h| &h.tree);
        let mut seen = BTreeSet::new();
        for talent in spec
            .trees
            .iter()
            .chain(hero)
            .flat_map(|t| &t.nodes)
            .flat_map(|n| &n.choices)
        {
            if !seen.insert(*talent) {
                c.issues.push(DataIssue::TalentInTwoTrees {
                    spec: spec.id,
                    talent: *talent,
                });
            }
        }
    }
    for (key, class) in &data.classes {
        let owner = Owner::Class(key.clone());
        if *key != class.name {
            c.issues.push(DataIssue::KeyMismatch(owner.clone()));
        }
        let pulled = class
            .on_pull
            .iter()
            .flat_map(|p| std::iter::once(p.aura).chain(p.lockout));
        for aura in class
            .group_auras
            .iter()
            .chain(&class.enemy_auras)
            .copied()
            .chain(pulled)
        {
            c.aura(&owner, aura);
        }
    }
    for hero in data.hero_trees.values() {
        let owner = Owner::HeroTree(hero.id);
        for &spec in &hero.specs {
            if !data.specs.contains_key(&spec) {
                c.issues.push(DataIssue::UnknownSpec {
                    owner: owner.clone(),
                    spec,
                });
            }
        }
        for talent in hero
            .tree
            .nodes
            .iter()
            .flat_map(|n| n.choices.iter().chain(&n.requires_any))
        {
            c.talent(&owner, *talent);
        }
        if !hero
            .tree
            .nodes
            .iter()
            .any(|n| n.choices.contains(&hero.keystone))
        {
            c.issues.push(DataIssue::KeystoneNotInTree(hero.id));
        }
    }
    for spell in data.spells.values() {
        let owner = Owner::Spell(spell.id);
        c.effects(&owner, &spell.effects);
        if spell.speed.is_some_and(|s| !positive(s)) {
            c.issues.push(DataIssue::InvalidSpeed(spell.id));
        }
        if spell.range.is_some_and(|r| !positive(r)) {
            c.issues.push(DataIssue::InvalidRange(spell.id));
        }
        if let CastKind::Empower {
            stages,
            stage_effects,
            ..
        } = &spell.cast
        {
            stage_effects.iter().for_each(|e| c.effects(&owner, e));
            let rising = stages.first().is_some_and(|&s| s > SimDuration::ZERO)
                && stages.windows(2).all(|w| w[0] < w[1]);
            if !rising || stage_effects.len() != stages.len() {
                c.issues.push(DataIssue::InvalidEmpower(spell.id));
            }
        }
        if let CastKind::Channel {
            duration, ticks, ..
        } = spell.cast
        {
            if ticks == 0 || duration == SimDuration::ZERO {
                c.issues.push(DataIssue::InvalidChannel(spell.id));
            }
        }
        for r in &spell.requires {
            let ok = match *r {
                Requirement::AnyAura(ref auras) | Requirement::NoAura(ref auras) => {
                    auras.iter().for_each(|&a| c.aura(&owner, a));
                    true
                }
                Requirement::OutOfCombat => true,
                Requirement::TargetHpAtMost(f) | Requirement::TargetHpAtLeast(f) => {
                    (0.0..=1.0).contains(&f)
                }
                Requirement::CasterStacksAtLeast { aura, stacks } => {
                    c.aura(&owner, aura);
                    stacks > 0
                }
            };
            if !ok {
                c.issues.push(DataIssue::InvalidRequirement(spell.id));
            }
        }
    }
    let mut categories: BTreeMap<u32, &CooldownDef> = BTreeMap::new();
    for cd in data.spells.values().filter_map(|s| s.cooldown.as_ref()) {
        let Some(category) = cd.category else {
            continue;
        };
        let first = *categories.entry(category).or_insert(cd);
        let agree =
            (first.duration, first.charges, first.hasted) == (cd.duration, cd.charges, cd.hasted);
        let issue = DataIssue::CategoryMismatch(category);
        if !agree && !c.issues.contains(&issue) {
            c.issues.push(issue);
        }
    }
    for aura in data.auras.values() {
        let owner = Owner::Aura(aura.id);
        if let Some(p) = &aura.periodic {
            c.effects(&owner, &p.effects);
        }
        if let Some(v) = &aura.value {
            c.effects(&owner, &v.on_threshold);
            let limits_ok = [v.initial, v.cap, v.threshold]
                .into_iter()
                .flatten()
                .all(|l| positive(factor(l)));
            let bank_ok = match v.kind {
                AuraValueKind::Bank(draw) => {
                    aura.periodic.is_some()
                        && match draw {
                            BankDraw::Fraction(f) => positive(f) && f <= 1.0,
                            BankDraw::SpreadOverRemaining => true,
                        }
                }
                AuraValueKind::Absorb { .. } | AuraValueKind::Counter => true,
            };
            if !limits_ok || !bank_ok {
                c.issues.push(DataIssue::InvalidAuraValue(aura.id));
            }
        }
        c.effects(&owner, &aura.on_expire);
        if let Some(p) = &aura.prevents_death {
            if !(positive(p.heal_to_pct) && p.heal_to_pct <= 1.0) {
                c.issues.push(DataIssue::InvalidDeathPrevention(aura.id));
            }
            if let Some(l) = p.lockout {
                c.aura(&owner, l);
            }
            c.effects(&owner, &p.on_prevent);
        }
        for m in &aura.modifiers {
            if let ModScope::Spell(s) = m.scope {
                c.spell(&owner, s);
            }
            if let Some(p) = &m.condition {
                c.predicate(&owner, p);
            }
        }
        aura.listeners.iter().for_each(|l| c.listener(&owner, l));
        c.shared_procs(&owner, &aura.listeners);
        for &(from, to) in &aura.overrides {
            c.spell(&owner, from);
            c.spell(&owner, to);
        }
        if let Some(a) = aura.blocked_by {
            c.aura(&owner, a);
        }
        if let Some(a) = aura.ends_with {
            c.aura(&owner, a);
        }
        if let Some(f) = &aura.form {
            f.allows.iter().for_each(|&s| c.spell(&owner, s));
        }
        if let Some(st) = &aura.stealth {
            st.keeps.iter().for_each(|&s| c.spell(&owner, s));
            c.effects(&owner, &st.on_break);
        }
    }
    for item in data.items.values() {
        let owner = Owner::Item(item.id);
        item.passive_auras.iter().for_each(|&a| c.aura(&owner, a));
        if let Some(s) = item.on_use {
            c.spell(&owner, s);
        }
        if let Some(set) = item.set {
            if !data.item_sets.contains_key(&set) {
                c.issues.push(DataIssue::UnknownItemSet { owner, set });
            }
        }
    }
    for set in data.item_sets.values() {
        let owner = Owner::ItemSet(set.id);
        set.bonuses.iter().for_each(|&(_, a)| c.aura(&owner, a));
    }
    for talent in data.talents.values() {
        let owner = Owner::Talent(talent.id);
        for grant in talent.ranks.iter().flatten() {
            match *grant {
                Grant::Spell(s) => c.spell(&owner, s),
                Grant::PassiveAura(a) => c.aura(&owner, a),
                Grant::ReplaceSpell { from, to } => {
                    c.spell(&owner, from);
                    c.spell(&owner, to);
                }
            }
        }
    }
    for pet in data.pets.values() {
        let owner = Owner::Pet(pet.id);
        pet.resources.iter().for_each(|r| c.resource(&owner, r));
        pet.autocast.iter().for_each(|&s| c.spell(&owner, s));
        pet.passive_auras.iter().for_each(|&a| c.aura(&owner, a));
    }
    for (&stat, curve) in &data.curves.ratings {
        let ordered = curve.diminishing.windows(2).all(|w| w[0].0 < w[1].0);
        let fractions = curve
            .diminishing
            .iter()
            .all(|&(_, f)| (0.0..=1.0).contains(&f));
        if curve.rating_per_pct <= 0.0 || !ordered || !fractions {
            c.issues.push(DataIssue::InvalidRatingCurve(stat));
        }
    }
    if !(0.0..=100.0).contains(&data.curves.dual_wield_miss_pct) {
        c.issues.push(DataIssue::InvalidMissChance);
    }
    c.issues
}

/// Enemy auras live in the game data, so both tables are needed.
pub fn check_enemy_data(enemies: &EnemyData, data: &GameData) -> Vec<DataIssue> {
    let mut c = Checker {
        data,
        issues: Vec::new(),
    };
    for (key, enemy) in &enemies.enemies {
        let owner = Owner::Enemy(key.clone());
        if &enemy.key != key {
            c.issues.push(DataIssue::KeyMismatch(owner.clone()));
        }
        if enemy.health == 0 {
            c.issues.push(DataIssue::InvalidHealth(key.clone()));
        }
        for rule in &enemy.rules {
            c.enemy_action(enemies, &owner, &rule.action);
        }
    }
    c.issues
}

struct Checker<'a> {
    data: &'a GameData,
    issues: Vec<DataIssue>,
}

impl Checker<'_> {
    fn keys(&mut self) {
        let d = self.data;
        let mismatched = d
            .specs
            .iter()
            .filter(|(k, v)| **k != v.id)
            .map(|(k, _)| Owner::Spec(*k))
            .chain(
                d.spells
                    .iter()
                    .filter(|(k, v)| **k != v.id)
                    .map(|(k, _)| Owner::Spell(*k)),
            )
            .chain(
                d.auras
                    .iter()
                    .filter(|(k, v)| **k != v.id)
                    .map(|(k, _)| Owner::Aura(*k)),
            )
            .chain(
                d.items
                    .iter()
                    .filter(|(k, v)| **k != v.id)
                    .map(|(k, _)| Owner::Item(*k)),
            )
            .chain(
                d.item_sets
                    .iter()
                    .filter(|(k, v)| **k != v.id)
                    .map(|(k, _)| Owner::ItemSet(*k)),
            )
            .chain(
                d.talents
                    .iter()
                    .filter(|(k, v)| **k != v.id)
                    .map(|(k, _)| Owner::Talent(*k)),
            )
            .chain(
                d.hero_trees
                    .iter()
                    .filter(|(k, v)| **k != v.id)
                    .map(|(k, _)| Owner::HeroTree(*k)),
            )
            .chain(
                d.pets
                    .iter()
                    .filter(|(k, v)| **k != v.id)
                    .map(|(k, _)| Owner::Pet(*k)),
            );
        self.issues.extend(mismatched.map(DataIssue::KeyMismatch));
    }

    fn spell(&mut self, owner: &Owner, spell: SpellId) {
        if !self.data.spells.contains_key(&spell) {
            self.issues.push(DataIssue::UnknownSpell {
                owner: owner.clone(),
                spell,
            });
        }
    }

    fn aura(&mut self, owner: &Owner, aura: AuraId) {
        if !self.data.auras.contains_key(&aura) {
            self.issues.push(DataIssue::UnknownAura {
                owner: owner.clone(),
                aura,
            });
        }
    }

    fn pet(&mut self, owner: &Owner, pet: PetId) {
        if !self.data.pets.contains_key(&pet) {
            self.issues.push(DataIssue::UnknownPet {
                owner: owner.clone(),
                pet,
            });
        }
    }

    fn talent(&mut self, owner: &Owner, talent: TalentId) {
        if !self.data.talents.contains_key(&talent) {
            self.issues.push(DataIssue::UnknownTalent {
                owner: owner.clone(),
                talent,
            });
        }
    }

    fn target(&mut self, owner: &Owner, target: &EffectTarget) {
        match *target {
            EffectTarget::Pets(Some(p)) => self.pet(owner, p),
            EffectTarget::EnemiesWithAura { aura, .. }
            | EffectTarget::AlliesWithAura { aura, .. } => self.aura(owner, aura),
            _ => {}
        }
    }

    fn predicate(&mut self, owner: &Owner, p: &Predicate) {
        match *p {
            Predicate::TargetHasAura { aura, .. }
            | Predicate::CasterHasAura(aura)
            | Predicate::CasterLacksAura(aura)
            | Predicate::OwnerHasAura(aura) => {
                self.aura(owner, aura);
            }
            Predicate::TargetStacksAtLeast { aura, stacks, .. }
            | Predicate::CasterStacksAtLeast { aura, stacks } => {
                self.aura(owner, aura);
                self.valid_predicate(owner, stacks > 0);
            }
            Predicate::AuraValueAtLeast { aura, value } => {
                self.aura(owner, aura);
                self.valid_predicate(owner, value.is_finite());
            }
            Predicate::TargetHpBelow(f)
            | Predicate::TargetHpAbove(f)
            | Predicate::CasterHpBelow(f)
            | Predicate::CasterHpAbove(f) => {
                self.valid_predicate(owner, (0.0..=1.0).contains(&f));
            }
            Predicate::RecentCasts { spell, count } => {
                self.spell(owner, spell);
                self.valid_predicate(owner, (1..=RECENT_CASTS).contains(&usize::from(count)));
            }
            Predicate::DiffersFromLastCast | Predicate::TargetHpBelowCasterMaxHp => {}
        }
    }

    fn resource(&mut self, owner: &Owner, r: &ResourceDef) {
        if let Some(rate) = r.out_of_combat {
            if !rate.is_finite() || r.recharge.is_some() {
                self.issues
                    .push(DataIssue::InvalidOutOfCombat(owner.clone()));
            }
        }
        let Some(rc) = r.recharge else { return };
        let whole = |x: f64| x.is_finite() && x >= 0.0 && x.fract() == 0.0;
        let ok = r.regen_per_sec == 0.0
            && rc.concurrent > 0
            && rc.period > SimDuration::ZERO
            && whole(r.max)
            && whole(r.initial)
            && r.initial <= r.max;
        if !ok {
            self.issues.push(DataIssue::InvalidRecharge(owner.clone()));
        }
    }

    fn valid_predicate(&mut self, owner: &Owner, ok: bool) {
        if !ok {
            self.issues.push(DataIssue::InvalidPredicate(owner.clone()));
        }
    }

    fn listener(&mut self, owner: &Owner, l: &Listener) {
        match l.on {
            ListenFor::CastComplete { spell, .. }
            | ListenFor::DamageDealt { spell, .. }
            | ListenFor::CastStart { spell, .. } => {
                if let Some(s) = spell {
                    self.spell(owner, s);
                }
            }
            ListenFor::PeriodicTick(a)
            | ListenFor::AuraApplied(a)
            | ListenFor::AuraExpired(a)
            | ListenFor::Absorbed(a)
            | ListenFor::AuraRemovedBy { aura: a, .. } => {
                self.aura(owner, a);
            }
            ListenFor::AuraStacksReached { aura, stacks } => {
                self.aura(owner, aura);
                let max = self.data.auras.get(&aura).map_or(u8::MAX, |a| a.max_stacks);
                if stacks == 0 || stacks > max.max(1) {
                    self.issues.push(DataIssue::InvalidListener(owner.clone()));
                }
            }
            ListenFor::HealthBelow { pct } => {
                if !(1..=100).contains(&pct) {
                    self.issues.push(DataIssue::InvalidListener(owner.clone()));
                }
            }
            ListenFor::ResourceGained(_)
            | ListenFor::Interrupted
            | ListenFor::MoveStart
            | ListenFor::MoveEnd => {}
            ListenFor::PetExpired(p) => self.pet(owner, p),
            ListenFor::EnemyDied {
                had_aura: Some(a), ..
            } => self.aura(owner, a),
            ListenFor::EnemyDied { had_aura: None, .. }
            | ListenFor::DamageTaken
            | ListenFor::Swing { .. }
            | ListenFor::WeaponHit { .. }
            | ListenFor::ResourceSpent(_)
            | ListenFor::Departed => {}
        }
        if l.per_unit && !matches!(l.on, ListenFor::ResourceSpent(_)) {
            self.issues
                .push(DataIssue::PerUnitWithoutSpend(owner.clone()));
        }
        if let ProcChance::Deck { successes, size } = l.chance {
            if size == 0 || successes > size {
                self.issues.push(DataIssue::InvalidDeck(owner.clone()));
            }
        }
        if let Some(p) = &l.condition {
            self.predicate(owner, p);
        }
        self.effects(owner, &l.effects);
    }

    fn shared_procs(&mut self, owner: &Owner, listeners: &[Listener]) {
        for (index, l) in listeners.iter().enumerate() {
            let Some(with) = l.shared_with else {
                continue;
            };
            let valid = usize::from(with) < index
                && listeners
                    .get(usize::from(with))
                    .is_some_and(|w| w.shared_with.is_none() && w.chance == l.chance);
            if !valid {
                self.issues
                    .push(DataIssue::InvalidSharedProc(owner.clone()));
            }
        }
    }

    fn effects(&mut self, owner: &Owner, effects: &[Effect]) {
        for effect in effects {
            match effect {
                Effect::Damage {
                    target,
                    aoe,
                    per_count,
                    ..
                } => {
                    if aoe.is_some_and(|a| !(0.0..=1.0).contains(&a.secondary)) {
                        self.issues.push(DataIssue::InvalidAoeShare(owner.clone()));
                    }
                    if let Some(scale) = per_count {
                        if !scale.pct.is_finite() {
                            self.issues
                                .push(DataIssue::InvalidCountScale(owner.clone()));
                        }
                        if let TargetCount::EnemiesWithAura { aura, .. } = scale.count {
                            self.aura(owner, aura);
                        }
                    }
                    self.target(owner, target);
                }
                Effect::Heal { target, .. } | Effect::Interrupt { target } => {
                    self.target(owner, target);
                }
                Effect::ApplyAura {
                    aura,
                    target,
                    duration,
                    per_unit_spent: Some(per),
                    ..
                } => {
                    let timed = duration.is_some()
                        || self
                            .data
                            .auras
                            .get(aura)
                            .is_some_and(|a| a.duration.is_some());
                    if *per == SimDuration::ZERO || !timed {
                        self.issues
                            .push(DataIssue::InvalidSpentDuration(owner.clone()));
                    }
                    self.aura(owner, *aura);
                    self.target(owner, target);
                }
                Effect::ApplyAura { aura, target, .. }
                | Effect::RemoveAura { aura, target }
                | Effect::RemoveStacks { aura, target, .. }
                | Effect::ExtendAura { aura, target, .. }
                | Effect::AddAuraValue { aura, target, .. }
                | Effect::ConsumeAuraValue { aura, target, .. }
                | Effect::SpreadAura {
                    aura, to: target, ..
                } => {
                    self.aura(owner, *aura);
                    self.target(owner, target);
                }
                Effect::Summon { pet, .. } | Effect::Dismiss { pet, .. } => self.pet(owner, *pet),
                Effect::CommandPet { pet, spell } => {
                    if let Some(p) = pet {
                        self.pet(owner, *p);
                    }
                    self.spell(owner, *spell);
                }
                Effect::ExtendPets { pet, .. } => {
                    if let Some(p) = pet {
                        self.pet(owner, *p);
                    }
                }
                Effect::AdjustCooldown { spell, .. } => self.spell(owner, *spell),
                Effect::TriggerSpell { spell, target, .. } => {
                    self.spell(owner, *spell);
                    self.target(owner, target);
                }
                Effect::ApplyOneOf { auras, target } => {
                    if auras.is_empty() {
                        self.issues.push(DataIssue::EmptyChoice(owner.clone()));
                    }
                    auras.iter().for_each(|&a| self.aura(owner, a));
                    self.target(owner, target);
                }
                Effect::RandomOf(branches) => {
                    for (_, e) in branches {
                        self.effects(owner, std::slice::from_ref(e));
                    }
                }
                Effect::Chance { chance, then } => {
                    if !(0.0..=1.0).contains(chance) {
                        self.issues.push(DataIssue::InvalidChance(owner.clone()));
                    }
                    self.effects(owner, then);
                }
                Effect::If {
                    when,
                    then,
                    otherwise,
                } => {
                    self.predicate(owner, when);
                    self.effects(owner, then);
                    self.effects(owner, otherwise);
                }
                Effect::ForEach { target, then } => {
                    self.target(owner, target);
                    self.effects(owner, then);
                }
                Effect::Displace { yards, .. } => {
                    if !positive(*yards) {
                        self.issues.push(DataIssue::InvalidMovement(owner.clone()));
                    }
                }
                Effect::Resource(_)
                | Effect::GainResource { .. }
                | Effect::ExtraSwing
                | Effect::Hook(_) => {}
            }
        }
    }

    fn enemy_action(&mut self, enemies: &EnemyData, owner: &Owner, action: &EnemyAction) {
        match action {
            EnemyAction::Cast { then, .. } => self.enemy_action(enemies, owner, then),
            EnemyAction::Sequence(actions) => {
                for a in actions {
                    self.enemy_action(enemies, owner, a);
                }
            }
            EnemyAction::SelfAura(aura) => self.aura(owner, *aura),
            EnemyAction::SpawnAdds { adds, distance, .. } => {
                for (enemy, count) in adds {
                    match enemies.enemies.get(enemy) {
                        None => self.issues.push(DataIssue::UnknownEnemy {
                            owner: owner.clone(),
                            enemy: enemy.clone(),
                        }),
                        Some(def) if def.kind != EnemyKind::Add || *count == 0 => {
                            self.issues.push(DataIssue::InvalidAdds {
                                owner: owner.clone(),
                                enemy: enemy.clone(),
                            });
                        }
                        Some(_) => {}
                    }
                }
                if let Some(d) = distance {
                    let (lo, hi) = d.bounds();
                    if !(lo >= 0.0 && lo <= hi && hi.is_finite()) {
                        self.issues.push(DataIssue::InvalidMovement(owner.clone()));
                    }
                }
            }
            EnemyAction::MustMove {
                yards,
                within,
                on_fail,
                ..
            } => {
                if !positive(*yards) || *within == SimDuration::ZERO {
                    self.issues.push(DataIssue::InvalidMovement(owner.clone()));
                }
                self.effects(owner, on_fail);
            }
            EnemyAction::Reposition { distance } => {
                let (lo, hi) = distance.bounds();
                if !(lo >= 0.0 && lo <= hi && hi.is_finite()) {
                    self.issues.push(DataIssue::InvalidMovement(owner.clone()));
                }
            }
            EnemyAction::Knockback { yards, .. } => {
                if !positive(*yards) {
                    self.issues.push(DataIssue::InvalidMovement(owner.clone()));
                }
            }
            EnemyAction::Effects { effects, .. } => self.effects(owner, effects),
            EnemyAction::Damage { .. }
            | EnemyAction::ForceMovement { .. }
            | EnemyAction::EnterPhase(_) => {}
        }
    }
}
