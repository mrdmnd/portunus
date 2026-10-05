//! Stats, modifiers, crits, and mitigation.

use std::collections::BTreeMap;
use std::sync::Arc;

use portunus_core::{ActorId, Seat, SpecId, SpellId};
use portunus_engine::state::{ActorKind, AuraInstance};
use portunus_engine::{AuraRef, RunSetup, StateView};
use portunus_gamedata::effect::{Coefficient, ModKind, ModScope, Modifier, Predicate};
use portunus_gamedata::stats::{DerivedStats, RatedStat, SchoolMask, Stat, StatBlock};
use portunus_gamedata::GameData;
use portunus_scenario::resolved::Segment;

use crate::{CombatMath, EffectCtx, Outgoing};

#[derive(Debug)]
struct SeatStats {
    spec: SpecId,
    /// The template's stats, before any aura.
    stats: StatBlock,
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

    /// The spec of a seat, if the setup had one there.
    pub(crate) fn seat_spec(&self, seat: Seat) -> Option<SpecId> {
        self.s.seats.get(usize::from(seat.0)).map(|s| s.spec)
    }

    /// A seat's stats with every active aura applied; `None` for actors
    /// that aren't players.
    fn stat_block(&self, view: &dyn StateView, actor: ActorId) -> Option<(SpecId, StatBlock)> {
        let seat = player_seat(view, actor)?;
        let base = self.s.seats.get(usize::from(seat.0))?;
        let mut stats = base.stats.clone();
        let mut pct: BTreeMap<Stat, f64> = BTreeMap::new();
        self.each_mod(view, &Query::holder(actor), |m, value| match m.kind {
            ModKind::StatFlat(stat) => *stats.0.entry(stat).or_default() += value,
            ModKind::StatPct(stat) => *pct.entry(stat).or_insert(1.0) *= 1.0 + value / 100.0,
            _ => {}
        });
        for (stat, mult) in pct {
            if let Some(v) = stats.0.get_mut(&stat) {
                *v *= mult;
            }
        }
        Some((base.spec, stats))
    }

    /// Percentages and derived values with every active aura applied. Non-player
    /// actors have no stats beyond their health.
    pub fn derived(&self, view: &dyn StateView, actor: ActorId) -> DerivedStats {
        let spec = self
            .stat_block(view, actor)
            .and_then(|(spec, stats)| Some((self.s.data.specs.get(&spec)?, stats)));
        match spec {
            Some((spec, stats)) => portunus_loadout::derive(&self.s.data.curves, spec, &stats),
            None => DerivedStats {
                max_health: view.actor(actor).map_or(0.0, |a| a.max_health),
                ..DerivedStats::default()
            },
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
                .and_then(|(_, stats)| stats.0.get(&Stat::Armor).copied())
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

    fn spell_query(&self, ctx: &EffectCtx, school: Option<SchoolMask>) -> Query {
        Query {
            holder: ctx.caster,
            target: ctx.target,
            spell: ctx.spell,
            school: school.or_else(|| self.spell_school(ctx.spell)),
        }
    }
}

impl CombatMath for Formulas {
    fn base(&self, view: &dyn StateView, ctx: &EffectCtx, amount: Coefficient) -> f64 {
        let raw = match amount {
            Coefficient::Flat(x) => x,
            Coefficient::SpellPower(c) => c * self.derived(view, ctx.caster).spell_power,
            Coefficient::AttackPower(c) => c * self.derived(view, ctx.caster).attack_power,
            Coefficient::WeaponDamage(_) => 0.0,
            Coefficient::PctMaxHealth(pct) => {
                let who = ctx.target.unwrap_or(ctx.caster);
                pct / 100.0 * view.actor(who).map_or(0.0, |a| a.max_health)
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
            Outgoing::Damage(school) => self.mod_product(
                view,
                &self.spell_query(ctx, Some(school)),
                ModKind::DamageDonePct,
            ),
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
        let add = self.mod_sum(view, &self.spell_query(ctx, None), ModKind::CritChanceAdd);
        ((self.derived(view, ctx.caster).crit_pct + add) / 100.0).clamp(0.0, 1.0)
    }

    fn crit_multiplier(&self, view: &dyn StateView, ctx: &EffectCtx) -> f64 {
        let bonus = self.mod_sum(view, &self.spell_query(ctx, None), ModKind::CritDamagePct);
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

    fn haste_mult(&self, view: &dyn StateView, actor: ActorId) -> f64 {
        let rating = 1.0 + self.derived(view, actor).haste_pct / 100.0;
        rating * self.mod_product(view, &Query::holder(actor), ModKind::HastePct)
    }
}

pub(crate) fn player_seat(view: &dyn StateView, actor: ActorId) -> Option<Seat> {
    match view.actor(actor)?.kind {
        ActorKind::Player(seat) => Some(seat),
        ActorKind::Enemy { .. } | ActorKind::Pet { .. } => None,
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
        Predicate::TargetHpBelow(frac) => target
            .and_then(|t| view.actor(t))
            .is_some_and(|t| t.max_health > 0.0 && t.health / t.max_health < frac),
        Predicate::DiffersFromLastCast => player_seat(view, caster)
            .and_then(|s| view.last_cast(s))
            .is_none_or(|last| Some(last.spell) != spell),
    }
}
