//! Reference checks for loaded tables: every id points at something, and
//! every table entry sits under its own id.

use portunus_core::{
    AuraId, EnemyKey, ItemId, ItemSetId, PetId, Sample, SpecId, SpellId, TalentId,
};
use portunus_gamedata::effect::{Effect, EffectTarget, ListenFor, Listener, ModScope, Predicate};
use portunus_gamedata::enemy::EnemyAction;
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::stats::Stat;
use portunus_gamedata::talent::Grant;
use portunus_gamedata::{EnemyData, GameData};

/// Where a reference was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Spec(SpecId),
    Spell(SpellId),
    Aura(AuraId),
    Item(ItemId),
    ItemSet(ItemSetId),
    Talent(TalentId),
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
    UnknownItemSet {
        owner: Owner,
        set: ItemSetId,
    },
    UnknownEnemy {
        owner: Owner,
        enemy: EnemyKey,
    },
    /// Non-positive rating per percent, or thresholds out of order or
    /// fractions outside `[0, 1]`.
    InvalidRatingCurve(Stat),
    /// An enemy's health bounds are reversed or not positive.
    InvalidHealth(EnemyKey),
}

pub fn check_game_data(data: &GameData) -> Vec<DataIssue> {
    let mut c = Checker {
        data,
        issues: Vec::new(),
    };
    c.keys();
    for spec in data.specs.values() {
        let owner = Owner::Spec(spec.id);
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
    }
    for spell in data.spells.values() {
        let owner = Owner::Spell(spell.id);
        c.effects(&owner, &spell.effects);
        if let CastKind::Empower { stage_effects, .. } = &spell.cast {
            stage_effects.iter().for_each(|e| c.effects(&owner, e));
        }
    }
    for aura in data.auras.values() {
        let owner = Owner::Aura(aura.id);
        if let Some(p) = &aura.periodic {
            c.effects(&owner, &p.effects);
        }
        if let Some(v) = &aura.value {
            c.effects(&owner, &v.on_threshold);
        }
        c.effects(&owner, &aura.on_expire);
        for m in &aura.modifiers {
            if let ModScope::Spell(s) = m.scope {
                c.spell(&owner, s);
            }
            if let Some(p) = &m.condition {
                c.predicate(&owner, p);
            }
        }
        aura.listeners.iter().for_each(|l| c.listener(&owner, l));
        for &(from, to) in &aura.overrides {
            c.spell(&owner, from);
            c.spell(&owner, to);
        }
        if let Some(a) = aura.blocked_by {
            c.aura(&owner, a);
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
        let (lo, hi) = enemy.health.bounds();
        if lo <= 0.0 || lo > hi {
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
        if let EffectTarget::Pets(Some(p)) = target {
            self.pet(owner, *p);
        }
    }

    fn predicate(&mut self, owner: &Owner, p: &Predicate) {
        match *p {
            Predicate::TargetHasAura { aura, .. } | Predicate::CasterHasAura(aura) => {
                self.aura(owner, aura);
            }
            Predicate::TargetHpBelow(_) | Predicate::DiffersFromLastCast => {}
        }
    }

    fn listener(&mut self, owner: &Owner, l: &Listener) {
        match l.on {
            ListenFor::CastComplete { spell, .. } | ListenFor::DamageDealt { spell, .. } => {
                if let Some(s) = spell {
                    self.spell(owner, s);
                }
            }
            ListenFor::PeriodicTick(a) | ListenFor::AuraApplied(a) | ListenFor::AuraExpired(a) => {
                self.aura(owner, a);
            }
            ListenFor::DamageTaken | ListenFor::Swing { .. } | ListenFor::ResourceSpent(_) => {}
        }
        self.effects(owner, &l.effects);
    }

    fn effects(&mut self, owner: &Owner, effects: &[Effect]) {
        for effect in effects {
            match effect {
                Effect::Damage { target, .. }
                | Effect::Heal { target, .. }
                | Effect::Interrupt { target } => self.target(owner, target),
                Effect::ApplyAura { aura, target, .. }
                | Effect::RemoveAura { aura, target }
                | Effect::RemoveStacks { aura, target, .. }
                | Effect::ExtendAura { aura, target, .. }
                | Effect::AddAuraValue { aura, target, .. }
                | Effect::ConsumeAuraValue { aura, target, .. } => {
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
                Effect::TriggerSpell { spell, target } => {
                    self.spell(owner, *spell);
                    self.target(owner, target);
                }
                Effect::RandomOf(branches) => {
                    for (_, e) in branches {
                        self.effects(owner, std::slice::from_ref(e));
                    }
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
                Effect::Resource(_) | Effect::Hook(_) => {}
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
            EnemyAction::SpawnAdds { adds } => {
                for (enemy, _) in adds {
                    if !enemies.enemies.contains_key(enemy) {
                        self.issues.push(DataIssue::UnknownEnemy {
                            owner: owner.clone(),
                            enemy: enemy.clone(),
                        });
                    }
                }
            }
            EnemyAction::Damage { .. }
            | EnemyAction::ForceMovement { .. }
            | EnemyAction::EnterPhase(_) => {}
        }
    }
}
