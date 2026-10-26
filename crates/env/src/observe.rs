//! What a seat is allowed to see.
//!
//! Observers are the only code given privileged state on a policy's
//! behalf. Under [`InfoSet::Realistic`] an observer must restrict itself to
//! what a player could know in game: visible health, cast bars, and auras,
//! plus the timeline priors — never sampled future timings or pending spawns. It must also
//! hide events the seat hasn't perceived yet (`StateView::unperceived`),
//! and present the legality mask as it looked before them.

use std::collections::BTreeSet;

use portunus_core::{
    ActorId, AuraId, EventName, Seat, SimDuration, SimTime, SpawnLabel, SpecId, SpellId,
};
use portunus_engine::state::ActorKind;
use portunus_engine::{ActionMask, Readiness, SegmentView, StateView, WakeReason};
use portunus_gamedata::effect::Effect;
use portunus_gamedata::enemy::RuleIndex;
use portunus_gamedata::spell::{CastKind, Requirement};
use portunus_gamedata::stats::ResourceKind;
use portunus_gamedata::talent::Grant;
use portunus_gamedata::{EnemyData, GameData};
use portunus_loadout::ActorTemplate;
use portunus_plan::{Anchor, PullPlan};
use portunus_scenario::resolved::{ResolvedCombat, Segment};
use portunus_scenario::{ResolvedRun, TimelinePriors};
use serde::{Deserialize, Serialize};

use crate::action_space::{spec_spells, DEFAULT_TARGET_SLOTS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InfoSet {
    /// Sees the sampled truth, including future enemy timings. An upper
    /// bound, and a training aid.
    Privileged,
    /// Sees only what a player could know.
    Realistic,
}

pub struct ObsContext<'a> {
    pub state: &'a dyn StateView,
    pub run: &'a ResolvedRun,
    pub data: &'a GameData,
    pub enemies: &'a EnemyData,
    /// Every seat's compiled configuration, index = seat. Talents, gear,
    /// and set pieces are inspectable in game, so realistic observers may
    /// use them too.
    pub party: &'a [ActorTemplate],
    pub priors: &'a TimelinePriors,
    /// The plan for the pull in progress, if any.
    pub plan: Option<&'a PullPlan>,
    pub info: InfoSet,
}

pub trait Observer {
    /// Structured for scripted policies, or a flat feature vector for models.
    type Obs: Clone;
    fn observe(&self, ctx: &ObsContext<'_>, seat: Seat) -> Self::Obs;
    /// The legality mask the seat is shown, derived from the engine's.
    fn legal(&self, ctx: &ObsContext<'_>, seat: Seat, engine_mask: &ActionMask) -> ActionMask;
}

/// Expected timing of an enemy rule firing in the current pull, derived from
/// [`TimelinePriors`] and visible rule firing counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpcomingEventPrior {
    pub actor: ActorId,
    pub spawn: SpawnLabel,
    pub event: EventName,
    /// Expected time until the next firing of this rule.
    pub expected_in: SimDuration,
}

/// Active or upcoming timing of a [`portunus_plan::PlannedWindow`] assigned to a seat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWindowTiming {
    pub spell: SpellId,
    pub target: Option<Seat>,
    /// Until the window opens (`Some(SimDuration::ZERO)` once open; `None` if
    /// its anchor has no known timing yet).
    pub opens_in: Option<SimDuration>,
    /// Until the window closes (`SimDuration::ZERO` once expired; `None` if
    /// its anchor has no known timing yet).
    pub closes_in: Option<SimDuration>,
}

impl PlannedWindowTiming {
    /// Whether the current moment lies inside `[anchor + offset - tolerance, anchor + offset + tolerance)`.
    pub fn is_open(&self) -> bool {
        self.opens_in == Some(SimDuration::ZERO)
            && self.closes_in.is_some_and(|c| c > SimDuration::ZERO)
    }
}

fn combat_at(run: &ResolvedRun, index: u16) -> Option<&ResolvedCombat> {
    run.segments
        .iter()
        .filter_map(|s| match s {
            Segment::Combat(c) => Some(c),
            Segment::Travel { .. } => None,
        })
        .nth(usize::from(index))
}

/// Computes visible upcoming enemy event timings (from [`TimelinePriors`]) and
/// assigned [`PlannedWindowTiming`]s for `seat`.
pub fn observe_priors_and_windows(
    ctx: &ObsContext<'_>,
    seat: Seat,
    hidden_enemies: &[ActorId],
) -> (Vec<UpcomingEventPrior>, Vec<PlannedWindowTiming>) {
    if ctx.priors.events.is_empty() && ctx.plan.is_none_or(|p| p.windows.is_empty()) {
        return (Vec::new(), Vec::new());
    }
    let state = ctx.state;
    let now = state.now();
    let (combat, combat_started, travel_ends) = match state.segment() {
        SegmentView::Combat(c) => (combat_at(ctx.run, c.index), Some(c.started), None),
        SegmentView::Travel {
            next_combat,
            combat_starts,
            ..
        } => (combat_at(ctx.run, next_combat), None, Some(combat_starts)),
        SegmentView::Finished => (None, None, None),
    };

    let mut upcoming = Vec::new();
    if let (Some(combat), Some(_)) = (combat, combat_started) {
        for &e in state.enemies() {
            if hidden_enemies.contains(&e) {
                continue;
            }
            let Some(a) = state.actor(e) else { continue };
            if !(a.engaged && a.alive && a.targetable) {
                continue;
            }
            let ActorKind::Enemy { spawn: sp_idx, .. } = a.kind else {
                continue;
            };
            let Some(info) = state.enemy_info(e) else {
                continue;
            };
            let Some(def) = info.def else { continue };
            let Some(spawn) = combat.spawns.get(usize::from(sp_idx.0)) else {
                continue;
            };
            for (r, r_def) in def.rules.iter().enumerate() {
                let r_view = state.rule(e, RuleIndex(r as u16));
                if !r_view.active {
                    continue;
                }
                let prior = ctx.priors.events.iter().find(|p| {
                    p.pull == combat.pull && p.spawn == spawn.label && p.event == r_def.name
                });
                let expected_at = if r_view.fired == 0 {
                    info.engaged_at
                        .zip(prior.and_then(|p| p.first_mean))
                        .map(|(eng, fm)| eng + fm)
                } else {
                    r_view
                        .last_fired
                        .zip(prior.and_then(|p| p.interval_mean))
                        .map(|(lf, im)| lf + im)
                };
                if let Some(at) = expected_at {
                    upcoming.push(UpcomingEventPrior {
                        actor: e,
                        spawn: spawn.label.clone(),
                        event: r_def.name.clone(),
                        expected_in: at.saturating_since(now),
                    });
                }
            }
        }
        upcoming.sort_by_key(|u| u.expected_in);
    }

    let mut windows = Vec::new();
    if let Some(plan) = ctx.plan {
        for w in plan.windows.iter().filter(|w| w.seat == seat) {
            let anchor_at = match &w.anchor {
                Anchor::PullStart => combat_started.or(travel_ends),
                Anchor::SpawnEngaged(label) => combat.and_then(|c| {
                    state
                        .enemies()
                        .iter()
                        .copied()
                        .filter(|e| !hidden_enemies.contains(e))
                        .find_map(|e| {
                            let a = state.actor(e)?;
                            let ActorKind::Enemy { spawn: sp_idx, .. } = a.kind else {
                                return None;
                            };
                            let info = state.enemy_info(e)?;
                            let sp = c.spawns.get(usize::from(sp_idx.0))?;
                            (&sp.label == label).then_some(info.engaged_at)?
                        })
                }),
                Anchor::EnemyEvent {
                    spawn: label,
                    event,
                    nth,
                } => combat.and_then(|c| {
                    state
                        .enemies()
                        .iter()
                        .copied()
                        .filter(|e| !hidden_enemies.contains(e))
                        .find_map(|e| {
                            let a = state.actor(e)?;
                            let ActorKind::Enemy { spawn: sp_idx, .. } = a.kind else {
                                return None;
                            };
                            let info = state.enemy_info(e)?;
                            let sp = c.spawns.get(usize::from(sp_idx.0))?;
                            if &sp.label != label {
                                return None;
                            }
                            let def = info.def?;
                            let (r, _) = def
                                .rules
                                .iter()
                                .enumerate()
                                .find(|(_, rd)| &rd.name == event)?;
                            let r_view = state.rule(e, RuleIndex(r as u16));
                            if r_view.fired >= *nth {
                                return r_view.last_fired;
                            }
                            let eng = info.engaged_at?;
                            let prior = ctx.priors.events.iter().find(|p| {
                                p.pull == c.pull && &p.spawn == label && &p.event == event
                            })?;
                            if *nth == 1 {
                                prior.first_mean.map(|fm| eng + fm)
                            } else if r_view.fired > 0 {
                                r_view.last_fired.zip(prior.interval_mean).map(|(lf, im)| {
                                    lf + SimDuration(im.0.saturating_mul(nth - r_view.fired))
                                })
                            } else {
                                prior.first_mean.zip(prior.interval_mean).map(|(fm, im)| {
                                    eng + fm + SimDuration(im.0.saturating_mul(nth - 1))
                                })
                            }
                        })
                }),
            };
            let (opens_in, closes_in) = match anchor_at {
                Some(at) => {
                    let center = i64::from(at.0) + i64::from(w.offset.0);
                    let tol = i64::from(w.tolerance.0);
                    let start = SimTime((center - tol).max(0) as u32);
                    let end = SimTime((center + tol).max(0) as u32);
                    (
                        Some(start.saturating_since(now)),
                        Some(end.saturating_since(now)),
                    )
                }
                None => (None, None),
            };
            windows.push(PlannedWindowTiming {
                spell: w.spell,
                target: w.target,
                opens_in,
                closes_in,
            });
        }
    }

    (upcoming, windows)
}

/// Every timed aura referenced by `spec`'s spells, talents, or baseline
/// auras, in sorted `AuraId` order.
pub fn spec_auras(data: &GameData, spec: SpecId) -> Option<Vec<AuraId>> {
    fn walk_effects(effects: &[Effect], out: &mut BTreeSet<AuraId>) {
        for e in effects {
            match e {
                Effect::ApplyAura { aura, .. }
                | Effect::RemoveAura { aura, .. }
                | Effect::RemoveStacks { aura, .. }
                | Effect::ExtendAura { aura, .. }
                | Effect::SpreadAura { aura, .. }
                | Effect::TickAura { aura, .. }
                | Effect::AddAuraValue { aura, .. }
                | Effect::ConsumeAuraValue { aura, .. } => {
                    out.insert(*aura);
                }
                Effect::ConsumeStacks { aura, then, .. } => {
                    out.insert(*aura);
                    walk_effects(then, out);
                }
                Effect::If {
                    then, otherwise, ..
                } => {
                    walk_effects(then, out);
                    walk_effects(otherwise, out);
                }
                Effect::Chance { then, .. } | Effect::ForEach { then, .. } => {
                    walk_effects(then, out);
                }
                Effect::RandomOf(branches) => {
                    for (_, branch) in branches {
                        walk_effects(std::slice::from_ref(branch), out);
                    }
                }
                Effect::HighestSecondary {
                    crit,
                    haste,
                    mastery,
                    versatility,
                } => {
                    for list in [crit, haste, mastery, versatility] {
                        walk_effects(list, out);
                    }
                }
                _ => {}
            }
        }
    }

    let spec_def = data.specs.get(&spec)?;
    let spells = spec_spells(data, spec)?;
    let mut auras: BTreeSet<AuraId> = spec_def.baseline_auras.iter().copied().collect();
    let hero_trees = data
        .hero_trees
        .values()
        .filter(|h| h.specs.contains(&spec))
        .map(|h| &h.tree);
    for node in spec_def
        .trees
        .iter()
        .chain(hero_trees)
        .flat_map(|t| &t.nodes)
    {
        for &tid in &node.choices {
            if let Some(t) = data.talents.get(&tid) {
                for rank in &t.ranks {
                    for eff in rank {
                        if let Grant::PassiveAura(a) = eff {
                            auras.insert(*a);
                        }
                    }
                }
            }
        }
    }
    for spell_id in &spells {
        if let Some(spell) = data.spells.get(spell_id) {
            for req in &spell.requires {
                match req {
                    Requirement::AnyAura(list) | Requirement::NoAura(list) => {
                        auras.extend(list.iter().copied());
                    }
                    Requirement::CasterStacksAtLeast { aura, .. } => {
                        auras.insert(*aura);
                    }
                    _ => {}
                }
            }
            walk_effects(&spell.on_cast, &mut auras);
            walk_effects(&spell.effects, &mut auras);
            match &spell.cast {
                CastKind::Channel { on_end, .. } => walk_effects(on_end, &mut auras),
                CastKind::Empower { stage_effects, .. } => {
                    for stage in stage_effects {
                        walk_effects(stage, &mut auras);
                    }
                }
                _ => {}
            }
        }
    }
    let snapshot: Vec<AuraId> = auras.iter().copied().collect();
    for aura_id in snapshot {
        if let Some(def) = data.auras.get(&aura_id) {
            if let Some(p) = &def.periodic {
                walk_effects(&p.effects, &mut auras);
            }
            for l in &def.listeners {
                walk_effects(&l.effects, &mut auras);
            }
            walk_effects(&def.on_expire, &mut auras);
        }
    }
    let mut applied = BTreeSet::new();
    for spell_id in &spells {
        if let Some(spell) = data.spells.get(spell_id) {
            walk_effects(&spell.on_cast, &mut applied);
            walk_effects(&spell.effects, &mut applied);
        }
    }
    Some(
        auras
            .into_iter()
            .filter(|a| {
                applied.contains(a) || data.auras.get(a).is_some_and(|d| d.duration.is_some())
            })
            .collect(),
    )
}

/// A flat feature-vector [`Observer`] (`Obs = Vec<f32>`) for a specialization.
///
/// Respects [`InfoSet::Realistic`] by hiding unperceived aura applications,
/// cooldown recoveries, newly engaged enemies, and movement demands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatObserver {
    pub spec: SpecId,
    pub resources: Vec<ResourceKind>,
    pub spells: Vec<SpellId>,
    pub auras: Vec<AuraId>,
}

impl FlatObserver {
    /// Builds the canonical flat observer for `spec` from `data`.
    pub fn for_spec(data: &GameData, spec: SpecId) -> Option<Self> {
        let spec_def = data.specs.get(&spec)?;
        let resources = spec_def.resources.iter().map(|r| r.kind).collect();
        let spells = spec_spells(data, spec)?;
        let auras = spec_auras(data, spec)?;
        Some(Self {
            spec,
            resources,
            spells,
            auras,
        })
    }

    /// Fixed feature-vector length produced by [`Self::observe`].
    pub fn width(&self) -> usize {
        10 + DEFAULT_TARGET_SLOTS
            + 2 * self.resources.len()
            + 2 * self.spells.len()
            + 4 * self.auras.len()
    }
}

impl Observer for FlatObserver {
    type Obs = Vec<f32>;

    fn observe(&self, ctx: &ObsContext<'_>, seat: Seat) -> Vec<f32> {
        let state = ctx.state;
        let now = state.now();
        let me = state.seats()[usize::from(seat.0)];
        let mut hidden_auras = Vec::new();
        let mut hidden_cds = Vec::new();
        let mut hidden_enemies = Vec::new();
        if ctx.info == InfoSet::Realistic {
            for p in state.unperceived(seat) {
                match p.reason {
                    WakeReason::AuraGained(a) => hidden_auras.push(a),
                    WakeReason::CooldownReady(s) => hidden_cds.push(s),
                    WakeReason::EnemyEngaged(e) => hidden_enemies.push(e),
                    _ => {}
                }
            }
        }

        let mut out = Vec::with_capacity(self.width());
        let total_combats = ctx
            .run
            .segments
            .iter()
            .filter(|s| matches!(s, portunus_scenario::resolved::Segment::Combat(_)))
            .count();
        let (combat_sec, pulls_rem) = match state.segment() {
            SegmentView::Combat(c) => (
                now.saturating_since(c.started).millis() as f32 / 1000.0,
                total_combats.saturating_sub(usize::from(c.index) + 1),
            ),
            SegmentView::Travel { next_combat, .. } => {
                (0.0, total_combats.saturating_sub(usize::from(next_combat)))
            }
            SegmentView::Finished => (0.0, 0),
        };
        let gcd_sec = state
            .gcd_end(seat)
            .map_or(0.0, |t| t.saturating_since(now).millis() as f32 / 1000.0);
        let health_frac = state.actor(me).map_or(1.0, |a| a.health_frac() as f32);
        let moving = if state.movement(seat).is_some() {
            1.0
        } else {
            0.0
        };
        let mut visible_enemies: Vec<_> = Vec::new();
        for &e in state.enemies() {
            if hidden_enemies.contains(&e) {
                continue;
            }
            if let Some(a) = state.actor(e) {
                if a.engaged && a.alive && a.targetable {
                    visible_enemies.push(e);
                }
            }
        }
        let target = state.target(me).filter(|t| visible_enemies.contains(t));
        let (target_hp, target_abs_hp, target_dist) = target
            .and_then(|t| {
                let a = state.actor(t)?;
                let dist = state.distance(seat, t).unwrap_or(0.0) as f32;
                let abs_hp = (a.health as f64 / 50_000_000.0).clamp(0.0, 2.0) as f32;
                Some((
                    a.health_frac() as f32,
                    abs_hp,
                    (dist / 40.0).clamp(0.0, 2.0),
                ))
            })
            .unwrap_or((0.0, 0.0, 0.0));
        let pulls_rem_norm = (pulls_rem as f32 / 5.0).clamp(0.0, 2.0);

        // 10 global & current-target scalars + 8 retarget-slot nameplate healths
        out.push(if matches!(state.segment(), SegmentView::Combat(_)) {
            1.0
        } else {
            0.0
        });
        out.push((combat_sec / 300.0).clamp(0.0, 2.0));
        out.push((gcd_sec / 1.5).clamp(0.0, 1.0));
        out.push(health_frac);
        out.push(moving);
        out.push((visible_enemies.len() as f32 / 10.0).clamp(0.0, 2.0));
        out.push(target_hp);
        out.push(target_abs_hp);
        out.push(target_dist);
        out.push(pulls_rem_norm);

        let mut other_iter = visible_enemies
            .iter()
            .copied()
            .filter(|&e| Some(e) != target);
        for _ in 0..DEFAULT_TARGET_SLOTS {
            let hp = other_iter
                .next()
                .and_then(|e| state.actor(e))
                .map_or(0.0, |a| {
                    (a.health as f64 / 50_000_000.0).clamp(0.0, 2.0) as f32
                });
            out.push(hp);
        }

        // 2 per resource: normalized current and deficit
        for &kind in &self.resources {
            if let Some(rv) = state.resource(me, kind) {
                let max = rv.max.max(1.0);
                let cur = (rv.value / max).clamp(0.0, 1.0) as f32;
                out.push(cur);
                out.push(1.0 - cur);
            } else {
                out.push(0.0);
                out.push(0.0);
            }
        }

        // 2 per spell: charges and remaining cooldown (normalized to 60s)
        for &spell in &self.spells {
            if hidden_cds.contains(&spell) {
                out.push(0.0);
                out.push(0.1);
            } else if let Some(cd) = state.cooldown(me, spell) {
                let rem = if cd.charges == 0 {
                    cd.next_charge_at
                        .map_or(0.0, |t| t.saturating_since(now).millis() as f32 / 60_000.0)
                } else {
                    0.0
                };
                out.push(f32::from(cd.charges));
                out.push(rem.clamp(0.0, 2.0));
            } else {
                out.push(1.0);
                out.push(0.0);
            }
        }

        // 4 per aura: caster (stacks, remaining/30s) and target (stacks, remaining/30s)
        let my_auras = state.auras(me);
        let target_auras = target.map(|t| state.auras(t)).unwrap_or(&[]);
        for &aura in &self.auras {
            if hidden_auras.contains(&aura) {
                out.push(0.0);
                out.push(0.0);
            } else if let Some(inst) = my_auras.iter().find(|i| i.aura == aura) {
                let rem = inst
                    .expires
                    .map_or(1.0, |e| e.saturating_since(now).millis() as f32 / 30_000.0);
                out.push(f32::from(inst.stacks) / 10.0);
                out.push(rem.clamp(0.0, 2.0));
            } else {
                out.push(0.0);
                out.push(0.0);
            }

            if let Some(inst) = target_auras
                .iter()
                .find(|i| i.aura == aura && i.source == me)
            {
                let rem = inst
                    .expires
                    .map_or(1.0, |e| e.saturating_since(now).millis() as f32 / 30_000.0);
                out.push(f32::from(inst.stacks) / 10.0);
                out.push(rem.clamp(0.0, 2.0));
            } else {
                out.push(0.0);
                out.push(0.0);
            }
        }

        debug_assert_eq!(out.len(), self.width());
        out
    }

    fn legal(&self, ctx: &ObsContext<'_>, seat: Seat, engine_mask: &ActionMask) -> ActionMask {
        let me = ctx.state.seats()[usize::from(seat.0)];
        let cur_target = ctx.state.target(me);
        let mut mask = engine_mask.clone();
        mask.targets.retain(|&t| Some(t) != cur_target);
        if ctx.info == InfoSet::Privileged {
            return mask;
        }
        for p in ctx.state.unperceived(seat) {
            match p.reason {
                WakeReason::CooldownReady(s) => {
                    if let Some(r) = mask.abilities.get_mut(&s) {
                        *r = Readiness::In(SimDuration(1));
                    }
                }
                WakeReason::AuraGained(a) => {
                    mask.cancel.retain(|&c| c != a);
                    for (&s, r) in &mut mask.abilities {
                        let needs_a = ctx.data.spells.get(&s).is_some_and(|def| {
                            def.requires.iter().any(|req| match req {
                                Requirement::AnyAura(list) => list.contains(&a),
                                Requirement::CasterStacksAtLeast { aura, .. } => *aura == a,
                                _ => false,
                            })
                        });
                        if needs_a {
                            *r = Readiness::Blocked;
                        }
                    }
                }
                WakeReason::EnemyEngaged(e) => {
                    mask.targets.retain(|&t| t != e);
                }
                _ => {}
            }
        }
        mask
    }
}
