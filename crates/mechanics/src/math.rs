//! Stats, modifiers, crits, and mitigation.

use std::collections::BTreeMap;
use std::sync::Arc;

use portunus_core::{ActorId, PetId, Seat, SpecId, SpellId};
use portunus_engine::state::{ActorKind, AuraInstance};
use portunus_engine::{AuraRef, RunSetup, StateView};
use portunus_gamedata::effect::{Coefficient, ModKind, ModScope, Modifier, Predicate};
use portunus_gamedata::item::{WeaponDef, WeaponHand};
use portunus_gamedata::pet::PetKind;
use portunus_gamedata::stats::{DerivedStats, RatedStat, SchoolMask, Stat, StatBlock};
use portunus_gamedata::GameData;
use portunus_scenario::resolved::Segment;

use crate::{CombatMath, EffectCtx, Outgoing};

/// Attack power per point of weapon DPS.
const WEAPON_AP_DIVISOR: f64 = 6.0;
const OFF_HAND_MULT: f64 = 0.5;

#[derive(Debug)]
struct SeatStats {
    spec: SpecId,
    /// The template's stats, before any aura.
    stats: StatBlock,
    weapons: [Option<WeaponDef>; 2],
}

/// A seat's stats at one moment, with its auras applied.
struct SeatSnapshot {
    spec: SpecId,
    stats: StatBlock,
    /// `RatedPct` points by stat, added after rating conversion.
    rated: BTreeMap<RatedStat, f64>,
}

/// Whose stats and modifiers an actor's actions use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Players and enemies: their own.
    Own,
    /// Totems act as their owner.
    Totem { owner: ActorId },
    /// Pets and guardians inherit some of the owner's and take the owner's
    /// pet or guardian damage modifiers.
    Pet { owner: ActorId, guardian: bool },
}

#[derive(Debug)]
struct Statics {
    data: Arc<GameData>,
    seats: Vec<SeatStats>,
    /// Armor by combat index, then spawn index.
    enemy_armor: Vec<Vec<f64>>,
}

/// The [`CombatMath`] implementation.
///
/// A seat's stats are its template's, plus `StatFlat` modifiers, times
/// `StatPct` modifiers, converted by `portunus_loadout::derive`. Amounts
/// stack in SimC's layers: coefficient, then `DamageDonePct` modifiers
/// (multiplicative with each other), then versatility, then the aura's
/// snapshotted persistent multiplier. Crit chance and crit damage bonuses
/// add up within their kind.
///
/// Pets and guardians use their own auras, plus the owner's
/// `PetDamagePct` or `GuardianDamagePct` and unscoped `CritChanceAdd`
/// modifiers; totems use the owner's stats and auras outright (see
/// [`PetScaling`](portunus_gamedata::pet::PetScaling)).
#[derive(Debug, Clone)]
pub struct Formulas {
    s: Arc<Statics>,
}

/// Which modifiers apply: those on `holder`'s auras whose scope matches
/// `spell` or `school` and whose condition holds.
#[derive(Debug, Clone, Copy)]
struct Query {
    holder: ActorId,
    target: Option<ActorId>,
    spell: Option<SpellId>,
    school: Option<SchoolMask>,
}

impl Query {
    fn holder(holder: ActorId) -> Self {
        Self {
            holder,
            target: None,
            spell: None,
            school: None,
        }
    }
}

impl Formulas {
    pub fn new(setup: &RunSetup) -> Self {
        let seats = setup
            .seats
            .iter()
            .map(|s| SeatStats {
                spec: s.template.spec,
                stats: s.template.stats.clone(),
                weapons: [s.template.main_hand, s.template.off_hand],
            })
            .collect();
        let enemy_armor = setup
            .run
            .segments
            .iter()
            .filter_map(|seg| match seg {
                Segment::Combat(c) => Some(&c.spawns),
                Segment::Travel { .. } => None,
            })
            .map(|spawns| {
                spawns
                    .iter()
                    .map(|s| {
                        setup
                            .enemies
                            .enemies
                            .get(&s.enemy)
                            .map_or(0.0, |d| d.defense.armor)
                    })
                    .collect()
            })
            .collect();
        Self {
            s: Arc::new(Statics {
                data: Arc::clone(&setup.data),
                seats,
                enemy_armor,
            }),
        }
    }

    pub(crate) fn data(&self) -> &GameData {
        &self.s.data
    }

    fn role(&self, view: &dyn StateView, actor: ActorId) -> Role {
        let Some(ActorKind::Pet { owner, pet }) = view.actor(actor).map(|a| a.kind) else {
            return Role::Own;
        };
        let Some(&owner) = view.seats().get(usize::from(owner.0)) else {
            return Role::Own;
        };
        match self.s.data.pets.get(&pet).map(|d| d.kind) {
            Some(PetKind::Totem) => Role::Totem { owner },
            Some(PetKind::Pet) => Role::Pet {
                owner,
                guardian: false,
            },
            Some(PetKind::Guardian) | None => Role::Pet {
                owner,
                guardian: true,
            },
        }
    }

    /// Whose auras modify `actor`'s actions: a totem's owner's, else its
    /// own.
    fn modifier_holder(&self, view: &dyn StateView, actor: ActorId) -> ActorId {
        match self.role(view, actor) {
            Role::Totem { owner } => owner,
            Role::Own | Role::Pet { .. } => actor,
        }
    }

    /// An equipped weapon: a seat's from its template, a pet's from its
    /// definition (main hand only).
    pub fn weapon(
        &self,
        view: &dyn StateView,
        actor: ActorId,
        hand: WeaponHand,
    ) -> Option<WeaponDef> {
        let i = match hand {
            WeaponHand::MainHand => 0,
            WeaponHand::OffHand => 1,
        };
        match view.actor(actor)?.kind {
            ActorKind::Player(seat) => self.s.seats.get(usize::from(seat.0))?.weapons[i],
            ActorKind::Pet { pet, .. } if i == 0 => self.s.data.pets.get(&pet)?.melee,
            ActorKind::Pet { .. } | ActorKind::Enemy { .. } => None,
        }
    }

    /// A white hit before modifiers: average weapon damage plus attack
    /// power over 6 per second of weapon speed, halved for the off hand.
    pub fn weapon_damage(&self, view: &dyn StateView, actor: ActorId, hand: WeaponHand) -> f64 {
        let Some(w) = self.weapon(view, actor, hand) else {
            return 0.0;
        };
        let speed = f64::from(w.speed.millis()) / 1000.0;
        let ap = self.derived(view, actor).attack_power;
        let hit = (w.min_damage + w.max_damage) / 2.0 + ap / WEAPON_AP_DIVISOR * speed;
        match hand {
            WeaponHand::MainHand => hit,
            WeaponHand::OffHand => hit * OFF_HAND_MULT,
        }
    }

    /// The product of `AttackSpeedPct` modifiers on the actor's own auras.
    pub fn attack_speed_mult(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        self.mod_product(view, &Query::holder(actor), ModKind::AttackSpeedPct)
    }

    /// The spec of a seat, if the setup had one there.
    pub(crate) fn seat_spec(&self, seat: Seat) -> Option<SpecId> {
        self.s.seats.get(usize::from(seat.0)).map(|s| s.spec)
    }

    /// A seat's stats with every active aura applied, and its `RatedPct`
    /// additions; `None` for actors that aren't players.
    fn stat_block(&self, view: &dyn StateView, actor: ActorId) -> Option<SeatSnapshot> {
        let seat = player_seat(view, actor)?;
        let base = self.s.seats.get(usize::from(seat.0))?;
        let mut stats = base.stats.clone();
        let mut pct: BTreeMap<Stat, f64> = BTreeMap::new();
        let mut rated: BTreeMap<RatedStat, f64> = BTreeMap::new();
        self.each_mod(view, &Query::holder(actor), |m, value| match m.kind {
            ModKind::StatFlat(stat) => *stats.0.entry(stat).or_default() += value,
            ModKind::StatPct(stat) => *pct.entry(stat).or_insert(1.0) *= 1.0 + value / 100.0,
            ModKind::RatedPct(stat) => *rated.entry(stat).or_default() += value,
            _ => {}
        });
        for (stat, mult) in pct {
            if let Some(v) = stats.0.get_mut(&stat) {
                *v *= mult;
            }
        }
        Some(SeatSnapshot {
            spec: base.spec,
            stats,
            rated,
        })
    }

    /// Percentages and derived values with every active aura applied. Pets
    /// take theirs from their owner's (see [`PetScaling`]); enemies have no
    /// stats beyond their health.
    ///
    /// [`PetScaling`]: portunus_gamedata::pet::PetScaling
    pub fn derived(&self, view: &dyn StateView, actor: ActorId) -> DerivedStats {
        if let Some(ActorKind::Pet { owner, pet }) = view.actor(actor).map(|a| a.kind) {
            if let Role::Totem { owner } = self.role(view, actor) {
                return DerivedStats {
                    max_health: view.actor(actor).map_or(0.0, |a| a.max_health as f64),
                    ..self.derived(view, owner)
                };
            }
            return self.pet_derived(view, actor, owner, pet);
        }
        let spec = self
            .stat_block(view, actor)
            .and_then(|s| Some((self.s.data.specs.get(&s.spec)?, s)));
        match spec {
            Some((spec, s)) => {
                let mut d = portunus_loadout::derive(&self.s.data.curves, spec, &s.stats);
                for (stat, points) in s.rated {
                    match stat {
                        RatedStat::Crit => d.crit_pct += points,
                        RatedStat::Haste => d.haste_pct += points,
                        RatedStat::Mastery => d.mastery_pct += points * spec.mastery_coef,
                        RatedStat::Versatility => d.versatility_pct += points,
                    }
                }
                d
            }
            None => DerivedStats {
                max_health: view.actor(actor).map_or(0.0, |a| a.max_health as f64),
                ..DerivedStats::default()
            },
        }
    }

    fn pet_derived(
        &self,
        view: &dyn StateView,
        actor: ActorId,
        owner: Seat,
        pet: PetId,
    ) -> DerivedStats {
        let k = self
            .s
            .data
            .pets
            .get(&pet)
            .map(|d| d.scaling)
            .unwrap_or_default();
        let o = view
            .seats()
            .get(usize::from(owner.0))
            .map(|&id| self.derived(view, id))
            .unwrap_or_default();
        DerivedStats {
            crit_pct: o.crit_pct,
            haste_pct: o.haste_pct,
            mastery_pct: 0.0,
            versatility_pct: o.versatility_pct,
            attack_power: o.attack_power * k.ap_from_ap + o.spell_power * k.ap_from_sp,
            spell_power: o.spell_power * k.sp_from_sp + o.attack_power * k.sp_from_ap,
            max_health: view.actor(actor).map_or(0.0, |a| a.max_health as f64),
        }
    }

    /// A rated secondary in percent.
    pub fn rated_pct(&self, view: &dyn StateView, actor: ActorId, stat: RatedStat) -> f64 {
        let d = self.derived(view, actor);
        match stat {
            RatedStat::Crit => d.crit_pct,
            RatedStat::Haste => d.haste_pct,
            RatedStat::Mastery => d.mastery_pct,
            RatedStat::Versatility => d.versatility_pct,
        }
    }

    fn armor(&self, view: &dyn StateView, target: ActorId) -> f64 {
        match view.actor(target).map(|a| a.kind) {
            Some(ActorKind::Enemy { combat, spawn }) => self
                .s
                .enemy_armor
                .get(usize::from(combat))
                .and_then(|c| c.get(usize::from(spawn.0)))
                .copied()
                .unwrap_or(0.0),
            Some(ActorKind::Player(_)) => self
                .stat_block(view, target)
                .and_then(|s| s.stats.0.get(&Stat::Armor).copied())
                .unwrap_or(0.0),
            Some(ActorKind::Pet { .. }) | None => 0.0,
        }
    }

    fn spell_school(&self, spell: Option<SpellId>) -> Option<SchoolMask> {
        spell
            .and_then(|s| self.s.data.spells.get(&s))
            .map(|d| d.school)
    }

    /// Calls `f` with every applicable modifier and its value times stacks
    /// (for `per_stack` modifiers).
    fn each_mod(&self, view: &dyn StateView, q: &Query, mut f: impl FnMut(&Modifier, f64)) {
        for inst in view.auras(q.holder) {
            let Some(def) = self.s.data.auras.get(&inst.aura) else {
                continue;
            };
            for m in &def.modifiers {
                if !self.applies(view, q, m) {
                    continue;
                }
                let stacks = if m.per_stack {
                    f64::from(inst.stacks)
                } else {
                    1.0
                };
                f(m, m.value * stacks);
            }
        }
    }

    fn applies(&self, view: &dyn StateView, q: &Query, m: &Modifier) -> bool {
        let in_scope = match m.scope {
            ModScope::All => true,
            ModScope::Spell(s) => q.spell == Some(s),
            ModScope::School(mask) => q.school.is_some_and(|s| s.0 & mask.0 != 0),
        };
        in_scope
            && m.condition
                .is_none_or(|p| predicate(view, p, q.holder, q.target, q.spell))
    }

    fn mod_sum(&self, view: &dyn StateView, q: &Query, kind: ModKind) -> f64 {
        let mut total = 0.0;
        self.each_mod(view, q, |m, v| {
            if m.kind == kind {
                total += v;
            }
        });
        total
    }

    /// The product of `1 + value / 100` over matching modifiers.
    fn mod_product(&self, view: &dyn StateView, q: &Query, kind: ModKind) -> f64 {
        let mut total = 1.0;
        self.each_mod(view, q, |m, v| {
            if m.kind == kind {
                total *= 1.0 + v / 100.0;
            }
        });
        total
    }

    fn mod_any(&self, view: &dyn StateView, q: &Query, kind: ModKind) -> bool {
        let mut any = false;
        self.each_mod(view, q, |m, _| any |= m.kind == kind);
        any
    }

    fn spell_query(
        &self,
        view: &dyn StateView,
        ctx: &EffectCtx,
        school: Option<SchoolMask>,
    ) -> Query {
        Query {
            holder: self.modifier_holder(view, ctx.caster),
            target: ctx.target,
            spell: ctx.spell,
            school: school.or_else(|| self.spell_school(ctx.spell)),
        }
    }

    /// The owner's pet or guardian damage multiplier for a pet's action;
    /// 1 for everyone else.
    fn owner_pet_mult(&self, view: &dyn StateView, ctx: &EffectCtx, school: SchoolMask) -> f64 {
        let Role::Pet { owner, guardian } = self.role(view, ctx.caster) else {
            return 1.0;
        };
        let q = Query {
            holder: owner,
            ..self.spell_query(view, ctx, Some(school))
        };
        let kind = if guardian {
            ModKind::GuardianDamagePct
        } else {
            ModKind::PetDamagePct
        };
        self.mod_product(view, &q, kind)
    }

    /// Crit chance as a fraction, before clamping to `[0, 1]`.
    fn uncapped_crit(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64 {
        let add = self.mod_sum(
            view,
            &self.spell_query(view, ctx, None),
            ModKind::CritChanceAdd,
        ) + self.owner_crit_add(view, ctx.caster);
        (self.derived(view, ctx.caster).crit_pct + add) / 100.0
    }

    /// Crit the owner's unscoped modifiers grant, which pets inherit.
    fn owner_crit_add(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        match self.role(view, actor) {
            Role::Pet { owner, .. } => {
                self.mod_sum(view, &Query::holder(owner), ModKind::CritChanceAdd)
            }
            Role::Own | Role::Totem { .. } => 0.0,
        }
    }
}

impl CombatMath for Formulas {
    fn base(&self, view: &dyn StateView, ctx: &EffectCtx, amount: Coefficient) -> f64 {
        let raw = match amount {
            Coefficient::Flat(x) => x,
            Coefficient::SpellPower(c) => c * self.derived(view, ctx.caster).spell_power,
            Coefficient::AttackPower(c) => c * self.derived(view, ctx.caster).attack_power,
            Coefficient::WeaponDamage(c) => {
                c * self.weapon_damage(view, ctx.caster, WeaponHand::MainHand)
            }
            Coefficient::PctMaxHealth(pct) => {
                let who = ctx.target.unwrap_or(ctx.caster);
                pct / 100.0 * view.actor(who).map_or(0.0, |a| a.max_health as f64)
            }
            Coefficient::EventAmount(f) => f * ctx.event_amount.unwrap_or(0.0),
            Coefficient::AuraValue(f) => {
                f * ctx
                    .aura
                    .and_then(|r| instance(view, r))
                    .map_or(0.0, |i| i.value)
            }
        };
        raw * ctx.scale
    }

    fn outgoing(
        &self,
        view: &dyn StateView,
        ctx: &EffectCtx,
        amount: Coefficient,
        kind: Outgoing,
    ) -> f64 {
        let base = self.base(view, ctx, amount);
        let done = match kind {
            Outgoing::Damage(school) => {
                let q = self.spell_query(view, ctx, Some(school));
                let mut by_crit = if self.mod_any(view, &q, ModKind::CritChanceScalesDamage) {
                    self.uncapped_crit(view, ctx).max(0.0)
                } else {
                    1.0
                };
                if self.mod_any(view, &q, ModKind::CritChanceAddsDamage) {
                    let untargeted = EffectCtx {
                        target: None,
                        ..*ctx
                    };
                    by_crit *= 1.0 + self.uncapped_crit(view, &untargeted).max(0.0);
                }
                self.mod_product(view, &q, ModKind::DamageDonePct)
                    * self.owner_pet_mult(view, ctx, school)
                    * by_crit
            }
            Outgoing::Heal => 1.0,
        };
        let vers = 1.0 + self.derived(view, ctx.caster).versatility_pct / 100.0;
        let pmult = ctx
            .aura
            .and_then(|r| instance(view, r))
            .map_or(1.0, |i| i.pmultiplier);
        base * done * vers * pmult
    }

    fn crit_chance(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64 {
        self.uncapped_crit(view, ctx).clamp(0.0, 1.0)
    }

    fn crit_multiplier(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64 {
        let bonus = self.mod_sum(
            view,
            &self.spell_query(view, ctx, None),
            ModKind::CritDamagePct,
        );
        2.0 * (1.0 + bonus / 100.0)
    }

    fn mitigate(
        &self,
        view: &dyn StateView,
        target: ActorId,
        amount: f64,
        school: SchoolMask,
    ) -> f64 {
        let q = Query {
            school: Some(school),
            ..Query::holder(target)
        };
        if self.mod_any(view, &q, ModKind::Immune) {
            return 0.0;
        }
        let mut out = amount * self.mod_product(view, &q, ModKind::DamageTakenPct);
        if school == SchoolMask::PHYSICAL {
            let armor = self.armor(view, target);
            if armor > 0.0 {
                out *= 1.0 - armor / (armor + self.s.data.curves.armor_constant);
            }
        }
        if player_seat(view, target).is_some() {
            out *= 1.0 - self.derived(view, target).versatility_pct / 200.0;
        }
        out.max(0.0)
    }

    /// Pets and guardians run at their owner's haste times their own
    /// `HastePct` modifiers; totems at their owner's.
    fn haste_mult(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        match self.role(view, actor) {
            Role::Totem { owner } => self.haste_mult(view, owner),
            Role::Pet { owner, .. } => {
                self.haste_mult(view, owner)
                    * self.mod_product(view, &Query::holder(actor), ModKind::HastePct)
            }
            Role::Own => {
                let rating = 1.0 + self.derived(view, actor).haste_pct / 100.0;
                rating * self.mod_product(view, &Query::holder(actor), ModKind::HastePct)
            }
        }
    }
}

pub(crate) fn player_seat(view: &dyn StateView, actor: ActorId) -> Option<Seat> {
    match view.actor(actor)?.kind {
        ActorKind::Player(seat) => Some(seat),
        ActorKind::Enemy { .. } | ActorKind::Pet { .. } => None,
    }
}

/// The seat behind a player or one of its pets.
pub(crate) fn owner_seat(view: &dyn StateView, actor: ActorId) -> Option<Seat> {
    match view.actor(actor)?.kind {
        ActorKind::Player(seat) | ActorKind::Pet { owner: seat, .. } => Some(seat),
        ActorKind::Enemy { .. } => None,
    }
}

pub(crate) fn instance(view: &dyn StateView, r: AuraRef) -> Option<&AuraInstance> {
    view.auras(r.holder)
        .iter()
        .find(|i| i.aura == r.aura && i.source == r.source)
}

pub(crate) fn predicate(
    view: &dyn StateView,
    p: Predicate,
    caster: ActorId,
    target: Option<ActorId>,
    spell: Option<SpellId>,
) -> bool {
    match p {
        Predicate::TargetHasAura { aura, from_self } => target.is_some_and(|t| {
            view.auras(t)
                .iter()
                .any(|i| i.aura == aura && (!from_self || i.source == caster))
        }),
        Predicate::CasterHasAura(aura) => view.auras(caster).iter().any(|i| i.aura == aura),
        Predicate::CasterLacksAura(aura) => !view.auras(caster).iter().any(|i| i.aura == aura),
        Predicate::OwnerHasAura(aura) => match view.actor(caster).map(|a| a.kind) {
            Some(ActorKind::Pet { owner, .. }) => view
                .seats()
                .get(usize::from(owner.0))
                .is_some_and(|&o| view.auras(o).iter().any(|i| i.aura == aura)),
            _ => false,
        },
        Predicate::TargetHpBelow(frac) => target
            .and_then(|t| view.actor(t))
            .is_some_and(|t| t.max_health > 0 && t.health_frac() < frac),
        Predicate::DiffersFromLastCast => player_seat(view, caster)
            .and_then(|s| view.last_cast(s))
            .is_none_or(|last| Some(last.spell) != spell),
    }
}
