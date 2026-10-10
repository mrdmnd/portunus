//! A structured observation for scripted policies.

use std::collections::BTreeMap;

use portunus_core::{ActorId, AuraId, HeroTreeId, PetId, Seat, SimDuration, SimTime, SpellId};
use portunus_engine::state::{ActorKind, AuraInstance, CastWhat};
use portunus_engine::{ActionMask, Readiness, SegmentView, WakeReason};
use portunus_env::{InfoSet, ObsContext, Observer};
use portunus_gamedata::pet::PetKind;
use portunus_gamedata::stats::ResourceKind;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuraObs {
    pub stacks: u8,
    /// `None` for permanent auras.
    pub remaining: Option<SimDuration>,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CooldownObs {
    pub charges: u8,
    /// Until a charge is available; zero while one is.
    pub remaining: SimDuration,
    /// Until the next charge returns, even with one available; zero when
    /// full.
    pub next_charge: SimDuration,
    /// Full recharge time at the current rate.
    pub recharge: SimDuration,
}

impl CooldownObs {
    /// Charges plus the recovered part of the next one: SimC's
    /// `charges_fractional`.
    pub fn fractional(&self) -> f64 {
        if self.next_charge == SimDuration::ZERO || self.recharge == SimDuration::ZERO {
            return f64::from(self.charges);
        }
        let left = f64::from(self.next_charge.millis()) / f64::from(self.recharge.millis());
        f64::from(self.charges) + (1.0 - left).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetObs {
    pub actor: ActorId,
    /// Percent of max health, 0 to 100.
    pub health_pct: f64,
    /// Yards away.
    pub distance: f64,
    /// This seat's own auras on the target.
    pub mine: BTreeMap<AuraId, AuraObs>,
}

/// Movement in progress.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementObs {
    /// Until it ends at the current speed.
    pub remaining: SimDuration,
    /// Forced movement can't be stopped.
    pub forced: bool,
}

/// A channel in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelObs {
    pub ticks_done: u8,
    pub ticks_total: u8,
    /// Until the next tick.
    pub next_tick: SimDuration,
}

/// An empower charging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmpowerObs {
    /// Stages reached so far (0 before the first).
    pub stage: u8,
    /// Until the next stage; `None` at the final one.
    pub next_stage: Option<SimDuration>,
}

/// Movement owed by a deadline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DemandObs {
    pub yards: f64,
    /// Until the deadline.
    pub remaining: SimDuration,
}

/// One seat's view of the moment, in game terms.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatObs {
    pub now: SimTime,
    /// The seat's hero tree: fixed for the run, for policies that play
    /// each tree differently.
    pub hero_tree: Option<HeroTreeId>,
    /// Time since the current pull started; `None` out of combat.
    pub combat_time: Option<SimDuration>,
    pub gcd_remaining: SimDuration,
    pub casting: Option<SpellId>,
    /// Until the current cast completes.
    pub cast_remaining: Option<SimDuration>,
    /// The current channel's progress.
    pub channel: Option<ChannelObs>,
    /// The current empower's progress.
    pub empower: Option<EmpowerObs>,
    /// Percent of max health, 0 to 100.
    pub health_pct: f64,
    pub movement: Option<MovementObs>,
    /// Soonest deadline first.
    pub demands: Vec<DemandObs>,
    /// Yards per second.
    pub run_speed: f64,
    pub resources: BTreeMap<ResourceKind, f64>,
    pub resource_max: BTreeMap<ResourceKind, f64>,
    /// Auras the seat holds, from any source.
    pub buffs: BTreeMap<AuraId, AuraObs>,
    /// One entry per ability with a cooldown.
    pub cooldowns: BTreeMap<SpellId, CooldownObs>,
    pub target: Option<TargetObs>,
    /// The controlled pet, if one is out.
    pub pet: Option<PetObs>,
    /// Remaining lifetime of each active guardian and totem, by type,
    /// oldest first (`None`: no time limit).
    pub guardians: BTreeMap<PetId, Vec<Option<SimDuration>>>,
}

/// A controlled pet, as its owner sees it on the pet frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PetObs {
    pub actor: ActorId,
    pub pet: PetId,
    pub casting: Option<SpellId>,
    pub resources: BTreeMap<ResourceKind, f64>,
    pub buffs: BTreeMap<AuraId, AuraObs>,
    /// Autocast spells with a cooldown.
    pub cooldowns: BTreeMap<SpellId, CooldownObs>,
}

impl PetObs {
    pub fn resource(&self, kind: ResourceKind) -> f64 {
        self.resources.get(&kind).copied().unwrap_or(0.0)
    }

    pub fn cooldown_remaining(&self, spell: SpellId) -> SimDuration {
        self.cooldowns
            .get(&spell)
            .map_or(SimDuration::ZERO, |c| c.remaining)
    }
}

/// Read by permanent auras' remaining-time queries.
pub const FOREVER: SimDuration = SimDuration(u32::MAX);

fn left(a: Option<&AuraObs>) -> SimDuration {
    a.map_or(SimDuration::ZERO, |a| a.remaining.unwrap_or(FOREVER))
}

/// Queries for policy code. Absent auras read as zero remaining and zero
/// stacks; permanent ones as [`FOREVER`].
impl SeatObs {
    pub fn resource(&self, kind: ResourceKind) -> f64 {
        self.resources.get(&kind).copied().unwrap_or(0.0)
    }

    /// How far below its maximum a resource is.
    pub fn resource_deficit(&self, kind: ResourceKind) -> f64 {
        self.resource_max.get(&kind).copied().unwrap_or(0.0) - self.resource(kind)
    }

    pub fn has_buff(&self, aura: AuraId) -> bool {
        self.buffs.contains_key(&aura)
    }

    pub fn buff_remaining(&self, aura: AuraId) -> SimDuration {
        left(self.buffs.get(&aura))
    }

    pub fn buff_stacks(&self, aura: AuraId) -> u8 {
        self.buffs.get(&aura).map_or(0, |a| a.stacks)
    }

    /// This seat's own aura on its target.
    pub fn target_aura(&self, aura: AuraId) -> Option<&AuraObs> {
        self.target.as_ref().and_then(|t| t.mine.get(&aura))
    }

    pub fn target_aura_remaining(&self, aura: AuraId) -> SimDuration {
        left(self.target_aura(aura))
    }

    /// 0 to 100; `None` with no target.
    pub fn target_health_pct(&self) -> Option<f64> {
        self.target.as_ref().map(|t| t.health_pct)
    }

    /// Until a charge is available; zero for abilities without a cooldown.
    pub fn cooldown_remaining(&self, ability: SpellId) -> SimDuration {
        self.cooldowns
            .get(&ability)
            .map_or(SimDuration::ZERO, |c| c.remaining)
    }

    pub fn charges(&self, ability: SpellId) -> u8 {
        self.cooldowns.get(&ability).map_or(0, |c| c.charges)
    }

    /// SimC's `charges_fractional`; zero for abilities without a cooldown.
    pub fn charges_fractional(&self, ability: SpellId) -> f64 {
        self.cooldowns
            .get(&ability)
            .map_or(0.0, CooldownObs::fractional)
    }

    /// Seconds to spare before running at full speed would still meet
    /// every demand (any movement counts toward all of them); negative if
    /// running now is already too late, `None` with nothing owed.
    pub fn move_slack(&self) -> Option<f64> {
        let speed = self.run_speed.max(f64::MIN_POSITIVE);
        self.demands
            .iter()
            .map(|d| f64::from(d.remaining.millis()) / 1000.0 - d.yards / speed)
            .min_by(f64::total_cmp)
    }

    /// Yards to the target; `None` with no target.
    pub fn target_distance(&self) -> Option<f64> {
        self.target.as_ref().map(|t| t.distance)
    }
}

/// Builds [`SeatObs`].
///
/// Under [`InfoSet::Realistic`], what the seat hasn't perceived yet is
/// hidden: auras it gained, cooldowns that came back (shown with no charge,
/// and blocked in the mask), enemies that engaged, and movement demands
/// just placed on it. Other unperceived events are not hidden yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScriptObserver;

/// What a realistic seat can't see yet.
#[derive(Debug, Default)]
struct Hidden {
    auras: Vec<AuraId>,
    cooldowns: Vec<SpellId>,
    enemies: Vec<ActorId>,
    /// When each unseen demand was placed.
    demands: Vec<SimTime>,
}

fn hidden(ctx: &ObsContext<'_>, seat: Seat) -> Hidden {
    let mut h = Hidden::default();
    if ctx.info == InfoSet::Privileged {
        return h;
    }
    for p in ctx.state.unperceived(seat) {
        match p.reason {
            WakeReason::AuraGained(a) => h.auras.push(a),
            WakeReason::CooldownReady(s) => h.cooldowns.push(s),
            WakeReason::EnemyEngaged(e) => h.enemies.push(e),
            WakeReason::MustMove => h.demands.push(p.event_at),
            _ => {}
        }
    }
    h
}

fn aura_obs(i: &AuraInstance, now: SimTime) -> AuraObs {
    AuraObs {
        stacks: i.stacks,
        remaining: i.expires.map(|e| e.saturating_since(now)),
        value: i.value,
    }
}

impl Observer for ScriptObserver {
    type Obs = SeatObs;

    fn observe(&self, ctx: &ObsContext<'_>, seat: Seat) -> SeatObs {
        let state = ctx.state;
        let hide = hidden(ctx, seat);
        let now = state.now();
        let me = state.seats()[usize::from(seat.0)];
        let template = ctx.party.get(usize::from(seat.0));
        let actor = state.actor(me);

        let views: Vec<_> = template
            .into_iter()
            .flat_map(|t| &t.resources)
            .filter_map(|r| Some((r.kind, state.resource(me, r.kind)?)))
            .collect();
        let resources = views.iter().map(|(k, v)| (*k, v.value)).collect();
        let resource_max = views.iter().map(|(k, v)| (*k, v.max)).collect();
        let buffs = state
            .auras(me)
            .iter()
            .filter(|i| !hide.auras.contains(&i.aura))
            .map(|i| (i.aura, aura_obs(i, now)))
            .collect();
        let cooldown = |actor: ActorId, s: SpellId| {
            let cd = state.cooldown(actor, s)?;
            let obs = if hide.cooldowns.contains(&s) {
                CooldownObs {
                    charges: 0,
                    remaining: SimDuration::ZERO,
                    next_charge: SimDuration::ZERO,
                    recharge: cd.recharge,
                }
            } else {
                let next_charge = cd
                    .next_charge_at
                    .map_or(SimDuration::ZERO, |t| t.saturating_since(now));
                CooldownObs {
                    charges: cd.charges,
                    remaining: if cd.charges == 0 {
                        next_charge
                    } else {
                        SimDuration::ZERO
                    },
                    next_charge,
                    recharge: cd.recharge,
                }
            };
            Some((s, obs))
        };
        let cooldowns = template
            .into_iter()
            .flat_map(|t| &t.abilities)
            .filter_map(|&s| cooldown(me, s))
            .collect();
        let mut pet = None;
        let mut guardians: BTreeMap<PetId, Vec<Option<SimDuration>>> = BTreeMap::new();
        for &id in state.pets(seat) {
            let Some(a) = state.actor(id) else { continue };
            let ActorKind::Pet { pet: kind, .. } = a.kind else {
                continue;
            };
            let Some(def) = ctx.data.pets.get(&kind) else {
                continue;
            };
            if def.kind != PetKind::Pet {
                guardians
                    .entry(kind)
                    .or_default()
                    .push(a.expires.map(|e| e.saturating_since(now)));
                continue;
            }
            pet = Some(PetObs {
                actor: id,
                pet: kind,
                casting: a.casting.and_then(|c| match c.what {
                    CastWhat::Spell(s) => Some(s),
                    CastWhat::EnemyRule(_) => None,
                }),
                resources: def
                    .resources
                    .iter()
                    .filter_map(|r| Some((r.kind, state.resource(id, r.kind)?.value)))
                    .collect(),
                buffs: state
                    .auras(id)
                    .iter()
                    .map(|i| (i.aura, aura_obs(i, now)))
                    .collect(),
                cooldowns: def
                    .autocast
                    .iter()
                    .filter_map(|&s| cooldown(id, s))
                    .collect(),
            });
        }
        let target = state
            .target(me)
            .filter(|t| !hide.enemies.contains(t))
            .and_then(|t| {
                let a = state.actor(t)?;
                Some(TargetObs {
                    actor: t,
                    health_pct: 100.0 * a.health_frac(),
                    distance: state.distance(seat, t).unwrap_or(0.0),
                    mine: state
                        .auras(t)
                        .iter()
                        .filter(|i| i.source == me)
                        .map(|i| (i.aura, aura_obs(i, now)))
                        .collect(),
                })
            });

        SeatObs {
            now,
            hero_tree: template.and_then(|t| t.hero_tree),
            combat_time: match state.segment() {
                SegmentView::Combat(c) => Some(now.saturating_since(c.started)),
                SegmentView::Travel { .. } | SegmentView::Finished => None,
            },
            gcd_remaining: state
                .gcd_end(seat)
                .map_or(SimDuration::ZERO, |g| g.saturating_since(now)),
            casting: actor.and_then(|a| a.casting).and_then(|c| match c.what {
                CastWhat::Spell(s) => Some(s),
                CastWhat::EnemyRule(_) => None,
            }),
            cast_remaining: actor
                .and_then(|a| a.casting)
                .map(|c| c.ends.saturating_since(now)),
            channel: actor.and_then(|a| a.casting).and_then(|c| {
                let p = c.ticks?;
                Some(ChannelObs {
                    ticks_done: p.done,
                    ticks_total: p.total,
                    next_tick: c.next_tick?.saturating_since(now),
                })
            }),
            empower: actor.and_then(|a| a.casting).and_then(|c| {
                Some(EmpowerObs {
                    stage: c.empower_stage?,
                    next_stage: c.next_stage_at.map(|t| t.saturating_since(now)),
                })
            }),
            health_pct: actor.map_or(0.0, |a| 100.0 * a.health_frac()),
            movement: state.movement(seat).map(|m| MovementObs {
                remaining: m.ends.saturating_since(now),
                forced: m.forced,
            }),
            demands: state
                .demands(seat)
                .into_iter()
                .filter(|d| !hide.demands.contains(&d.placed))
                .map(|d| DemandObs {
                    yards: d.yards,
                    remaining: d.deadline.saturating_since(now),
                })
                .collect(),
            run_speed: state.run_speed(seat),
            resources,
            resource_max,
            buffs,
            cooldowns,
            target,
            pet,
            guardians,
        }
    }

    fn legal(&self, ctx: &ObsContext<'_>, seat: Seat, engine_mask: &ActionMask) -> ActionMask {
        let hide = hidden(ctx, seat);
        let mut mask = engine_mask.clone();
        for s in &hide.cooldowns {
            if let Some(r) = mask.abilities.get_mut(s) {
                *r = Readiness::Blocked;
            }
        }
        mask.targets.retain(|t| !hide.enemies.contains(t));
        mask
    }
}
