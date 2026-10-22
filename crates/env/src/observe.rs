//! What a seat is allowed to see.
//!
//! Observers are the only code given privileged state on a policy's
//! behalf. Under [`InfoSet::Realistic`] an observer must restrict itself to
//! what a player could know in game (and what an addon can read from the
//! game API): visible health, cast bars, and auras, plus the timeline
//! priors — never sampled future timings or pending spawns. It must also
//! hide events the seat hasn't perceived yet (`StateView::unperceived`),
//! and present the legality mask as it looked before them.

use std::collections::BTreeSet;

use portunus_core::{AuraId, Seat, SimDuration, SpecId, SpellId};
use portunus_engine::{ActionMask, Readiness, SegmentView, StateView, WakeReason};
use portunus_gamedata::effect::Effect;
use portunus_gamedata::spell::{CastKind, Requirement};
use portunus_gamedata::stats::ResourceKind;
use portunus_gamedata::talent::Grant;
use portunus_gamedata::{EnemyData, GameData};
use portunus_loadout::ActorTemplate;
use portunus_plan::PullPlan;
use portunus_scenario::{ResolvedRun, TimelinePriors};
use serde::{Deserialize, Serialize};

use crate::action_space::spec_spells;

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
        9 + 2 * self.resources.len() + 2 * self.spells.len() + 4 * self.auras.len()
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
        let combat_sec = match state.segment() {
            SegmentView::Combat(c) => now.saturating_since(c.started).millis() as f32 / 1000.0,
            _ => 0.0,
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
        let visible_enemies: Vec<_> = state
            .enemies()
            .iter()
            .copied()
            .filter(|e| !hidden_enemies.contains(e))
            .filter(|&e| {
                state
                    .actor(e)
                    .is_some_and(|a| a.engaged && a.alive && a.targetable)
            })
            .collect();
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

        // 9 global & target scalars
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
        if ctx.info == InfoSet::Privileged {
            return engine_mask.clone();
        }
        let mut mask = engine_mask.clone();
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
