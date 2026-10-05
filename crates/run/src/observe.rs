//! A structured observation for scripted policies.

use std::collections::BTreeMap;

use portunus_core::{ActorId, AuraId, Seat, SimDuration, SimTime, SpellId};
use portunus_engine::state::{AuraInstance, CastWhat};
use portunus_engine::{ActionMask, Readiness, SegmentView, WakeReason};
use portunus_env::{InfoSet, ObsContext, Observer};
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct TargetObs {
    pub actor: ActorId,
    /// Percent of max health, 0 to 100.
    pub health_pct: f64,
    /// This seat's own auras on the target.
    pub mine: BTreeMap<AuraId, AuraObs>,
}

/// One seat's view of the moment, in game terms.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatObs {
    pub now: SimTime,
    /// Time since the current pull started; `None` out of combat.
    pub combat_time: Option<SimDuration>,
    pub gcd_remaining: SimDuration,
    pub casting: Option<SpellId>,
    pub resources: BTreeMap<ResourceKind, f64>,
    /// Auras the seat holds, from any source.
    pub buffs: BTreeMap<AuraId, AuraObs>,
    /// One entry per ability with a cooldown.
    pub cooldowns: BTreeMap<SpellId, CooldownObs>,
    pub target: Option<TargetObs>,
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
}

/// Builds [`SeatObs`].
///
/// Under [`InfoSet::Realistic`], what the seat hasn't perceived yet is
/// hidden: auras it gained, cooldowns that came back (shown with no charge,
/// and blocked in the mask), and enemies that engaged. Other unperceived
/// events are not hidden yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScriptObserver;

/// What a realistic seat can't see yet.
#[derive(Debug, Default)]
struct Hidden {
    auras: Vec<AuraId>,
    cooldowns: Vec<SpellId>,
    enemies: Vec<ActorId>,
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

        let resources = template
            .into_iter()
            .flat_map(|t| &t.resources)
            .filter_map(|r| Some((r.kind, state.resource(me, r.kind)?.value)))
            .collect();
        let buffs = state
            .auras(me)
            .iter()
            .filter(|i| !hide.auras.contains(&i.aura))
            .map(|i| (i.aura, aura_obs(i, now)))
            .collect();
        let cooldowns = template
            .into_iter()
            .flat_map(|t| &t.abilities)
            .filter_map(|&s| {
                let cd = state.cooldown(me, s)?;
                let obs = if hide.cooldowns.contains(&s) {
                    CooldownObs {
                        charges: 0,
                        remaining: SimDuration::ZERO,
                    }
                } else {
                    CooldownObs {
                        charges: cd.charges,
                        remaining: match (cd.charges, cd.next_charge_at) {
                            (0, Some(t)) => t.saturating_since(now),
                            _ => SimDuration::ZERO,
                        },
                    }
                };
                Some((s, obs))
            })
            .collect();
        let target = state
            .target(me)
            .filter(|t| !hide.enemies.contains(t))
            .and_then(|t| {
                let a = state.actor(t)?;
                Some(TargetObs {
                    actor: t,
                    health_pct: if a.max_health > 0.0 {
                        100.0 * a.health / a.max_health
                    } else {
                        0.0
                    },
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
            resources,
            buffs,
            cooldowns,
            target,
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
