//! A structured observation for scripted policies.

use std::collections::BTreeMap;

use portunus_core::{
    ActorId, AuraId, HeroTreeId, PetId, Seat, SimDuration, SimTime, SpecId, SpellId,
};
use portunus_engine::state::{ActorKind, AuraInstance, CastWhat};
use portunus_engine::{ActionMask, Readiness, SegmentView, WakeReason};
use portunus_env::{
    apply_interrupt_plan_mask, observe_duties, observe_priors_and_windows, spec_auras, spec_spells,
    DutyTiming, InfoSet, ObsContext, Observer, PlannedWindowTiming, UpcomingEventPrior,
    DEFAULT_TARGET_SLOTS,
};
use portunus_gamedata::aura::{AuraValueKind, DispelType};
use portunus_gamedata::pet::PetKind;
use portunus_gamedata::stats::ResourceKind;
use portunus_gamedata::GameData;
use portunus_policy::{Featurizer, SchemaId};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AuraObs {
    pub stacks: u8,
    /// `None` for permanent auras.
    pub remaining: Option<SimDuration>,
    pub value: f64,
    /// For a ground aura the seat placed on itself.
    pub ground: Option<GroundObs>,
}

/// A ground aura the seat placed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundObs {
    /// Yards the seat can still move before it is outside; zero once it
    /// has left.
    pub yards_left: f64,
    /// Engaged enemies the seat can see inside it.
    pub enemies: usize,
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
    /// Remaining health points.
    pub health: u64,
    /// Maximum health points.
    pub max_health: u64,
    /// Percent of max health, 0 to 100.
    pub health_pct: f64,
    /// Shields left, in percent of max health (as the health bar shows).
    pub absorb_pct: f64,
    /// Yards away.
    pub distance: f64,
    /// Yards from the target to each other engaged enemy the seat can
    /// see, nearest first: what an area centred on it would reach.
    pub neighbours: Vec<f64>,
    /// This seat's own auras on the target.
    pub mine: BTreeMap<AuraId, AuraObs>,
    /// Until its stun ends, while it is stunned.
    pub stunned: Option<SimDuration>,
    /// Auras enemies put on it that a dispel can take, as the game's
    /// frames highlight them.
    pub dispellable: BTreeMap<AuraId, DispelType>,
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

/// Another seat, as party frames, cooldown trackers and a damage meter
/// show it: as it is, with no reaction delay.
#[derive(Debug, Clone, PartialEq)]
pub struct AllyObs {
    pub seat: Seat,
    pub actor: ActorId,
    pub alive: bool,
    /// Percent of max health, 0 to 100.
    pub health_pct: f64,
    /// Damage done so far this run.
    pub damage_done: u64,
    /// Auras it holds, from any source.
    pub buffs: BTreeMap<AuraId, AuraObs>,
    /// One entry per ability with a cooldown.
    pub cooldowns: BTreeMap<SpellId, CooldownObs>,
    /// Dead, with a resurrection on its way to it.
    pub res_offered: bool,
}

/// The group's battle resurrection charges, as the UI shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BattleResObs {
    pub charges: u8,
    /// Until the next charge; `None` while full.
    pub next_in: Option<SimDuration>,
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
    /// The seat's last few completed casts, newest first (combo strikes,
    /// Steady Focus).
    pub recent_casts: Vec<SpellId>,
    /// Until the current cast completes.
    pub cast_remaining: Option<SimDuration>,
    /// The current channel's progress.
    pub channel: Option<ChannelObs>,
    /// The current empower's progress.
    pub empower: Option<EmpowerObs>,
    /// Percent of max health, 0 to 100.
    pub health_pct: f64,
    /// Shields left, in percent of max health.
    pub absorb_pct: f64,
    pub movement: Option<MovementObs>,
    /// Soonest deadline first.
    pub demands: Vec<DemandObs>,
    /// Yards per second.
    pub run_speed: f64,
    pub resources: BTreeMap<ResourceKind, f64>,
    pub resource_max: BTreeMap<ResourceKind, f64>,
    /// For resources that refill a unit at a time (runes): how long until
    /// each missing unit is back, soonest first.
    pub refills: BTreeMap<ResourceKind, Vec<SimDuration>>,
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
    /// The other seats, in seat order.
    pub allies: Vec<AllyObs>,
    pub battle_res: BattleResObs,
    /// Number of combat pulls remaining after the current one (`0` on the final
    /// pull of the scenario).
    pub pulls_remaining: u16,
    /// Remaining health of each visible engaged enemy other than `target`, in
    /// the order of `ActionMask::targets` (`ActionSlot::Retarget(i)`).
    pub other_target_healths: Vec<u64>,
    /// Upcoming enemy rule firings in the current pull, estimated from
    /// [`portunus_scenario::TimelinePriors`] and visible firing counts.
    pub upcoming_events: Vec<UpcomingEventPrior>,
    /// Planned cooldown windows assigned to this seat in the current (or
    /// upcoming) pull.
    pub planned_windows: Vec<PlannedWindowTiming>,
    /// Next assigned interrupt/CC duty for this seat from [`portunus_plan::PullPlan::interrupts`].
    pub next_kick_duty: Option<DutyTiming>,
    /// Next assigned personal or external defensive duty for this seat from
    /// [`portunus_plan::PullPlan::defensives`].
    pub next_defensive_duty: Option<DutyTiming>,
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

    /// How long until the seat has `units` of a resource that refills a unit
    /// at a time (SimC's `rune.time_to_N`): zero if it already does,
    /// [`FOREVER`] if it never will without help.
    pub fn time_to(&self, kind: ResourceKind, units: u32) -> SimDuration {
        let short = f64::from(units) - self.resource(kind);
        if short <= 0.0 {
            return SimDuration::ZERO;
        }
        self.refills
            .get(&kind)
            .and_then(|r| r.get(short.ceil() as usize - 1))
            .copied()
            .unwrap_or(FOREVER)
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

    /// Remaining health points of the current target; `None` with no target.
    pub fn target_health(&self) -> Option<u64> {
        self.target.as_ref().map(|t| t.health)
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

    /// Enemies an area of `yards` centred on the target would hit, the
    /// target included; zero with no target.
    pub fn enemies_near_target(&self, yards: f64) -> usize {
        self.target.as_ref().map_or(0, |t| {
            1 + t.neighbours.iter().take_while(|&&d| d <= yards).count()
        })
    }

    /// Whether a planned cooldown window for `spell` is currently open.
    pub fn window_open(&self, spell: SpellId) -> bool {
        self.planned_windows
            .iter()
            .any(|w| w.spell == spell && w.is_open())
    }

    /// Expected time until the next firing of an enemy rule named `event`
    /// (from [`portunus_scenario::TimelinePriors`]).
    pub fn next_event_in(&self, event: &str) -> Option<SimDuration> {
        self.upcoming_events
            .iter()
            .filter(|e| e.event.0 == event)
            .map(|e| e.expected_in)
            .min()
    }

    /// Active interrupt/CC duty right now (`opens_in == 0 && deadline_in > 0`).
    pub fn active_kick_duty(&self) -> Option<&DutyTiming> {
        self.next_kick_duty.as_ref().filter(|d| d.is_active())
    }

    /// Active defensive duty right now (`opens_in == 0 && deadline_in > 0`).
    pub fn active_defensive_duty(&self) -> Option<&DutyTiming> {
        self.next_defensive_duty.as_ref().filter(|d| d.is_active())
    }

    /// Whether the seat is currently casting or channeling a spell that will
    /// not finish before an active duty (`next_kick_duty` or `next_defensive_duty`)
    /// that requires not casting, meaning the seat must `Choice::StopCast` now.
    pub fn should_stop_cast_for_duty(&self) -> bool {
        let Some(cast_rem) = self.cast_remaining else {
            return false;
        };
        let is_channel = self.channel.is_some();
        [&self.next_kick_duty, &self.next_defensive_duty]
            .into_iter()
            .flatten()
            .any(|d| {
                d.is_active() && d.requires_not_casting && (is_channel || cast_rem >= d.deadline_in)
            })
    }

    /// Whether a proposed cast of `cast_time` (plus `buffer` safety margin)
    /// will complete before any upcoming kick or defensive duty assigned to
    /// this seat.
    pub fn cast_fits_before_duty(&self, cast_time: SimDuration, buffer: SimDuration) -> bool {
        let needed = SimDuration(cast_time.0.saturating_add(buffer.0));
        [&self.next_kick_duty, &self.next_defensive_duty]
            .into_iter()
            .flatten()
            .all(|d| {
                if d.requires_gcd || (d.requires_not_casting && cast_time > SimDuration::ZERO) {
                    d.deadline_in > needed
                } else {
                    true
                }
            })
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
        ground: None,
    }
}

/// The actor's absorbs left, in percent of its max health.
fn absorb_pct(ctx: &ObsContext<'_>, actor: ActorId) -> f64 {
    let Some(max) = ctx
        .state
        .actor(actor)
        .map(|a| a.max_health)
        .filter(|&m| m > 0)
    else {
        return 0.0;
    };
    let shields: f64 = ctx
        .state
        .auras(actor)
        .iter()
        .filter(|i| {
            i.value > 0.0
                && ctx
                    .data
                    .auras
                    .get(&i.aura)
                    .and_then(|d| d.value.as_ref())
                    .is_some_and(|v| matches!(v.kind, AuraValueKind::Absorb { .. }))
        })
        .map(|i| i.value)
        .sum();
    100.0 * shields / max as f64
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
        let refills = views
            .iter()
            .filter(|(_, v)| !v.next_ready.is_empty())
            .map(|(k, v)| {
                (
                    *k,
                    v.next_ready
                        .iter()
                        .map(|t| t.saturating_since(now))
                        .collect(),
                )
            })
            .collect();
        let visible_enemies = || {
            state
                .enemies()
                .iter()
                .copied()
                .filter(|e| !hide.enemies.contains(e))
                .filter(|&e| {
                    state
                        .actor(e)
                        .is_some_and(|a| a.engaged && a.alive && a.targetable)
                })
        };
        let buffs = state
            .auras(me)
            .iter()
            .filter(|i| !hide.auras.contains(&i.aura))
            .map(|i| {
                let ground = i.ground.filter(|_| i.source == me).map(|g| GroundObs {
                    yards_left: g.yards_left(state.yards_travelled(seat)).max(0.0),
                    enemies: visible_enemies()
                        .filter(|&e| state.pack_position(e).is_some_and(|at| g.reaches(at)))
                        .count(),
                });
                (
                    i.aura,
                    AuraObs {
                        ground,
                        ..aura_obs(i, now)
                    },
                )
            })
            .collect();
        let cooldown_hiding = |actor: ActorId, s: SpellId, hidden: &[SpellId]| {
            let cd = state.cooldown(actor, s)?;
            let obs = if hidden.contains(&s) {
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
        let cooldown = |actor: ActorId, s: SpellId| cooldown_hiding(actor, s, &hide.cooldowns);
        let allies = (0..state.seats().len())
            .map(|i| Seat(i as u8))
            .filter(|&s| s != seat)
            .filter_map(|s| {
                let actor = state.seats()[usize::from(s.0)];
                let a = state.actor(actor)?;
                Some(AllyObs {
                    seat: s,
                    actor,
                    alive: a.alive,
                    health_pct: 100.0 * a.health_frac(),
                    damage_done: state.damage_done(s),
                    buffs: state
                        .auras(actor)
                        .iter()
                        .map(|i| (i.aura, aura_obs(i, now)))
                        .collect(),
                    cooldowns: state
                        .abilities(s)
                        .iter()
                        .filter_map(|&sp| cooldown_hiding(actor, sp, &[]))
                        .collect(),
                    res_offered: state.res_offered(s),
                })
            })
            .collect();
        let cooldowns = state
            .abilities(seat)
            .iter()
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
                let mut neighbours: Vec<f64> =
                    state.pack_position(t).map_or_else(Vec::new, |(x, y)| {
                        visible_enemies()
                            .filter(|&e| e != t)
                            .filter_map(|e| state.pack_position(e))
                            .map(|(ex, ey)| (ex - x).hypot(ey - y))
                            .collect()
                    });
                neighbours.sort_by(f64::total_cmp);
                Some(TargetObs {
                    actor: t,
                    health: a.health,
                    max_health: a.max_health,
                    health_pct: 100.0 * a.health_frac(),
                    absorb_pct: absorb_pct(ctx, t),
                    distance: state.distance(seat, t).unwrap_or(0.0),
                    neighbours,
                    mine: state
                        .auras(t)
                        .iter()
                        .filter(|i| i.source == me)
                        .map(|i| (i.aura, aura_obs(i, now)))
                        .collect(),
                    stunned: a.stunned_until.map(|u| u.saturating_since(now)),
                    dispellable: state
                        .auras(t)
                        .iter()
                        .filter(|i| state.enemies().contains(&i.source))
                        .filter_map(|i| Some((i.aura, ctx.data.auras.get(&i.aura)?.dispel?)))
                        .collect(),
                })
            });

        let total_combats = ctx
            .run
            .segments
            .iter()
            .filter(|s| matches!(s, portunus_scenario::resolved::Segment::Combat(_)))
            .count();
        let pulls_remaining = match state.segment() {
            SegmentView::Combat(c) => total_combats.saturating_sub(usize::from(c.index) + 1) as u16,
            SegmentView::Travel { next_combat, .. } => {
                total_combats.saturating_sub(usize::from(next_combat)) as u16
            }
            SegmentView::Finished => 0,
        };
        let cur_target = state.target(me).filter(|t| !hide.enemies.contains(t));
        let other_target_healths: Vec<u64> = visible_enemies()
            .filter(|&e| Some(e) != cur_target)
            .filter_map(|e| Some(state.actor(e)?.health))
            .collect();
        let (upcoming_events, planned_windows) =
            observe_priors_and_windows(ctx, seat, &hide.enemies);
        let (next_kick_duty, next_defensive_duty) = observe_duties(ctx, seat, &hide.enemies);

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
            recent_casts: state
                .recent_casts(seat)
                .iter()
                .flatten()
                .map(|c| c.spell)
                .collect(),
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
            absorb_pct: absorb_pct(ctx, me),
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
            refills,
            buffs,
            cooldowns,
            target,
            pet,
            guardians,
            allies,
            battle_res: {
                let b = state.battle_res();
                BattleResObs {
                    charges: b.charges,
                    next_in: b.next_at.map(|t| t.saturating_since(now)),
                }
            },
            pulls_remaining,
            other_target_healths,
            upcoming_events,
            planned_windows,
            next_kick_duty,
            next_defensive_duty,
        }
    }

    fn legal(&self, ctx: &ObsContext<'_>, seat: Seat, engine_mask: &ActionMask) -> ActionMask {
        let hide = hidden(ctx, seat);
        let me = ctx.state.seats()[usize::from(seat.0)];
        let cur_target = ctx.state.target(me);
        let mut mask = engine_mask.clone();
        for s in &hide.cooldowns {
            if let Some(r) = mask.abilities.get_mut(s) {
                *r = Readiness::Blocked;
            }
        }
        mask.targets
            .retain(|&t| Some(t) != cur_target && !hide.enemies.contains(&t));
        apply_interrupt_plan_mask(ctx, seat, &hide.enemies, &mut mask);
        mask
    }
}

/// Encodes [`SeatObs`] into a fixed-width feature vector for `spec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatObsFeaturizer {
    pub schema: SchemaId,
    pub resources: Vec<ResourceKind>,
    pub spells: Vec<SpellId>,
    pub auras: Vec<AuraId>,
}

impl SeatObsFeaturizer {
    pub fn for_spec(data: &GameData, spec: SpecId) -> Option<Self> {
        let spec_def = data.specs.get(&spec)?;
        let resources = spec_def.resources.iter().map(|r| r.kind).collect();
        let spells = spec_spells(data, spec)?;
        let auras = spec_auras(data, spec)?;
        Some(Self {
            schema: SchemaId(format!("seat_obs:v4:spec_{}", spec.0)),
            resources,
            spells,
            auras,
        })
    }

    pub fn width(&self) -> usize {
        10 + DEFAULT_TARGET_SLOTS
            + 2 * self.resources.len()
            + 2 * self.spells.len()
            + 4 * self.auras.len()
    }
}

impl Featurizer<SeatObs> for SeatObsFeaturizer {
    fn schema(&self) -> &SchemaId {
        &self.schema
    }

    fn width(&self) -> usize {
        10 + DEFAULT_TARGET_SLOTS
            + 2 * self.resources.len()
            + 2 * self.spells.len()
            + 4 * self.auras.len()
    }

    fn featurize(&self, obs: &SeatObs, mask: &ActionMask) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.width());
        let in_combat = if obs.combat_time.is_some() { 1.0 } else { 0.0 };
        let combat_sec = obs.combat_time.map_or(0.0, |t| t.millis() as f32 / 1000.0);
        let gcd_sec = obs.gcd_remaining.millis() as f32 / 1000.0;
        let health_frac = (obs.health_pct / 100.0).clamp(0.0, 1.0) as f32;
        let moving = if obs.movement.is_some() { 1.0 } else { 0.0 };
        let (enemies, target_hp, target_abs_hp, target_dist) = match &obs.target {
            Some(t) => (
                ((1 + t.neighbours.len()) as f32 / 10.0).clamp(0.0, 2.0),
                (t.health_pct / 100.0).clamp(0.0, 1.0) as f32,
                (t.health as f64 / 50_000_000.0).clamp(0.0, 2.0) as f32,
                (t.distance as f32 / 40.0).clamp(0.0, 2.0),
            ),
            None => (0.0, 0.0, 0.0, 0.0),
        };
        let pulls_rem_norm = (f32::from(obs.pulls_remaining) / 5.0).clamp(0.0, 2.0);

        out.push(in_combat);
        out.push((combat_sec / 300.0).clamp(0.0, 2.0));
        out.push((gcd_sec / 1.5).clamp(0.0, 1.0));
        out.push(health_frac);
        out.push(moving);
        out.push(enemies);
        out.push(target_hp);
        out.push(target_abs_hp);
        out.push(target_dist);
        out.push(pulls_rem_norm);

        for i in 0..DEFAULT_TARGET_SLOTS {
            let hp = obs
                .other_target_healths
                .get(i)
                .map_or(0.0, |&h| (h as f64 / 50_000_000.0).clamp(0.0, 2.0) as f32);
            out.push(hp);
        }

        for &kind in &self.resources {
            let max = obs
                .resource_max
                .get(&kind)
                .copied()
                .unwrap_or(100.0)
                .max(1.0);
            let cur = (obs.resource(kind) / max).clamp(0.0, 1.0) as f32;
            out.push(cur);
            out.push(1.0 - cur);
        }

        for &spell in &self.spells {
            let ready = if mask.is_ready(spell) { 1.0 } else { 0.0 };
            let rem = (obs.cooldown_remaining(spell).millis() as f32 / 60_000.0).clamp(0.0, 2.0);
            out.push(ready);
            out.push(rem);
        }

        for &aura in &self.auras {
            let b_stacks = f32::from(obs.buff_stacks(aura)) / 10.0;
            let b_rem = obs.buffs.get(&aura).map_or(0.0, |a| {
                a.remaining
                    .map_or(1.0, |r| (r.millis() as f32 / 30_000.0).clamp(0.0, 2.0))
            });
            out.push(b_stacks);
            out.push(b_rem);

            let d_stacks = obs
                .target_aura(aura)
                .map_or(0.0, |a| f32::from(a.stacks) / 10.0);
            let d_rem = obs.target_aura(aura).map_or(0.0, |a| {
                a.remaining
                    .map_or(1.0, |r| (r.millis() as f32 / 30_000.0).clamp(0.0, 2.0))
            });
            out.push(d_stacks);
            out.push(d_rem);
        }

        debug_assert_eq!(out.len(), self.width());
        out
    }
}

impl Observer for SeatObsFeaturizer {
    type Obs = Vec<f32>;

    fn observe(&self, ctx: &ObsContext<'_>, seat: Seat) -> Vec<f32> {
        let state = ctx.state;
        let hide = hidden(ctx, seat);
        let now = state.now();
        let me = state.seats()[usize::from(seat.0)];
        let template = ctx.party.get(usize::from(seat.0));
        let actor = state.actor(me);

        let mut out = Vec::with_capacity(self.width());
        let total_combats = ctx
            .run
            .segments
            .iter()
            .filter(|s| matches!(s, portunus_scenario::resolved::Segment::Combat(_)))
            .count();
        let (in_combat, combat_sec, pulls_rem) = match state.segment() {
            SegmentView::Combat(c) => (
                1.0,
                now.saturating_since(c.started).millis() as f32 / 1000.0,
                total_combats.saturating_sub(usize::from(c.index) + 1),
            ),
            SegmentView::Travel { next_combat, .. } => (
                0.0,
                0.0,
                total_combats.saturating_sub(usize::from(next_combat)),
            ),
            SegmentView::Finished => (0.0, 0.0, 0),
        };
        let gcd_sec = state
            .gcd_end(seat)
            .map_or(0.0, |g| g.saturating_since(now).millis() as f32 / 1000.0);
        let health_frac = actor.map_or(0.0, |a| a.health_frac().clamp(0.0, 1.0) as f32);
        let moving = if state.movement(seat).is_some() {
            1.0
        } else {
            0.0
        };

        let target_id = state.target(me).filter(|t| !hide.enemies.contains(t));
        let (enemies, target_hp, target_abs_hp, target_dist) =
            match target_id.and_then(|t| Some((t, state.actor(t)?))) {
                Some((t, a)) => {
                    let n_neighbours = if state.pack_position(t).is_some() {
                        state
                            .enemies()
                            .iter()
                            .copied()
                            .filter(|&e| e != t && !hide.enemies.contains(&e))
                            .filter(|&e| {
                                state
                                    .actor(e)
                                    .is_some_and(|ea| ea.engaged && ea.alive && ea.targetable)
                                    && state.pack_position(e).is_some()
                            })
                            .count()
                    } else {
                        0
                    };
                    (
                        ((1 + n_neighbours) as f32 / 10.0).clamp(0.0, 2.0),
                        a.health_frac().clamp(0.0, 1.0) as f32,
                        (a.health as f64 / 50_000_000.0).clamp(0.0, 2.0) as f32,
                        (state.distance(seat, t).unwrap_or(0.0) as f32 / 40.0).clamp(0.0, 2.0),
                    )
                }
                None => (0.0, 0.0, 0.0, 0.0),
            };
        let pulls_rem_norm = (pulls_rem as f32 / 5.0).clamp(0.0, 2.0);

        out.push(in_combat);
        out.push((combat_sec / 300.0).clamp(0.0, 2.0));
        out.push((gcd_sec / 1.5).clamp(0.0, 1.0));
        out.push(health_frac);
        out.push(moving);
        out.push(enemies);
        out.push(target_hp);
        out.push(target_abs_hp);
        out.push(target_dist);
        out.push(pulls_rem_norm);

        let mut other_iter = state
            .enemies()
            .iter()
            .copied()
            .filter(|&e| Some(e) != target_id && !hide.enemies.contains(&e))
            .filter(|&e| {
                state
                    .actor(e)
                    .is_some_and(|ea| ea.engaged && ea.alive && ea.targetable)
            });
        for _ in 0..DEFAULT_TARGET_SLOTS {
            let hp = other_iter
                .next()
                .and_then(|e| state.actor(e))
                .map_or(0.0, |ea| {
                    (ea.health as f64 / 50_000_000.0).clamp(0.0, 2.0) as f32
                });
            out.push(hp);
        }

        for &kind in &self.resources {
            let has_res = template.is_some_and(|t| t.resources.iter().any(|r| r.kind == kind));
            let (val, max) = if has_res {
                state
                    .resource(me, kind)
                    .map_or((0.0, 100.0), |rv| (rv.value, rv.max.max(1.0)))
            } else {
                (0.0, 100.0)
            };
            let cur = (val / max).clamp(0.0, 1.0) as f32;
            out.push(cur);
            out.push(1.0 - cur);
        }

        let seat_abilities = state.abilities(seat);
        for &spell in &self.spells {
            let rem_ms = if !hide.cooldowns.contains(&spell) && seat_abilities.contains(&spell) {
                state.cooldown(me, spell).map_or(0, |cd| {
                    if cd.charges == 0 {
                        cd.next_charge_at
                            .map_or(0, |t| t.saturating_since(now).millis())
                    } else {
                        0
                    }
                })
            } else {
                0
            };
            out.push(0.0);
            out.push((rem_ms as f32 / 60_000.0).clamp(0.0, 2.0));
        }

        let me_auras = state.auras(me);
        let t_auras = target_id.map_or(&[][..], |t| state.auras(t));
        for &aura in &self.auras {
            let b_inst = if hide.auras.contains(&aura) {
                None
            } else {
                me_auras.iter().rev().find(|i| i.aura == aura)
            };
            let b_stacks = b_inst.map_or(0.0, |i| f32::from(i.stacks) / 10.0);
            let b_rem = b_inst.map_or(0.0, |i| {
                i.expires.map_or(1.0, |e| {
                    (e.saturating_since(now).millis() as f32 / 30_000.0).clamp(0.0, 2.0)
                })
            });
            out.push(b_stacks);
            out.push(b_rem);

            let d_inst = t_auras
                .iter()
                .rev()
                .find(|i| i.source == me && i.aura == aura);
            let d_stacks = d_inst.map_or(0.0, |i| f32::from(i.stacks) / 10.0);
            let d_rem = d_inst.map_or(0.0, |i| {
                i.expires.map_or(1.0, |e| {
                    (e.saturating_since(now).millis() as f32 / 30_000.0).clamp(0.0, 2.0)
                })
            });
            out.push(d_stacks);
            out.push(d_rem);
        }

        debug_assert_eq!(out.len(), self.width());
        out
    }

    fn legal(&self, ctx: &ObsContext<'_>, seat: Seat, engine_mask: &ActionMask) -> ActionMask {
        ScriptObserver.legal(ctx, seat, engine_mask)
    }
}

impl Featurizer<Vec<f32>> for SeatObsFeaturizer {
    fn schema(&self) -> &SchemaId {
        &self.schema
    }

    fn width(&self) -> usize {
        10 + DEFAULT_TARGET_SLOTS
            + 2 * self.resources.len()
            + 2 * self.spells.len()
            + 4 * self.auras.len()
    }

    fn featurize(&self, obs: &Vec<f32>, mask: &ActionMask) -> Vec<f32> {
        let mut out = obs.clone();
        let base = 10 + DEFAULT_TARGET_SLOTS + 2 * self.resources.len();
        for (i, &spell) in self.spells.iter().enumerate() {
            let idx = base + 2 * i;
            if idx < out.len() {
                out[idx] = if mask.is_ready(spell) { 1.0 } else { 0.0 };
            }
        }
        out
    }
}
