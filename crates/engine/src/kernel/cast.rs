//! Cast gates: readiness, costs, cooldowns and charges, and targets.

use portunus_core::{ActorId, Seat, SimTime, SpellId};
use portunus_gamedata::effect::{CooldownChange, ModKind};
use portunus_gamedata::spell::{CooldownDef, SpellDef, Targeting};
use portunus_gamedata::stats::{ResourceAmount, SpendScaling};

use crate::choice::TargetSel;
use crate::error::IllegalChoice;
use crate::mask::Readiness;
use crate::step::WakeReason;

use super::queue::Event;
use super::world::{millis_ceil, Cooldown, Seg, World};

/// The latest of several "not before" constraints.
pub(crate) struct Need {
    now: SimTime,
    blocked: bool,
    at: SimTime,
    reason: Option<WakeReason>,
}

impl Need {
    pub fn new(now: SimTime) -> Self {
        Self {
            now,
            blocked: false,
            at: now,
            reason: None,
        }
    }

    pub fn until(&mut self, t: SimTime, reason: WakeReason) {
        if t > self.at {
            self.at = t;
            self.reason = Some(reason);
        }
    }

    pub fn block(&mut self) {
        self.blocked = true;
    }

    pub fn add(&mut self, r: Readiness, reason: WakeReason) {
        match r {
            Readiness::Now => {}
            Readiness::In(d) => self.until(self.now + d, reason),
            Readiness::Blocked => self.block(),
        }
    }

    /// The readiness, and what to call the wake when it arrives.
    pub fn finish(self, ability: SpellId) -> (Readiness, WakeReason) {
        let reason = self.reason.unwrap_or(WakeReason::CooldownReady(ability));
        if self.blocked {
            (Readiness::Blocked, reason)
        } else if self.at > self.now {
            (Readiness::In(self.at - self.now), reason)
        } else {
            (Readiness::Now, reason)
        }
    }
}

impl World {
    /// A cooldown that has never been used: full, at current modifiers.
    pub(crate) fn fresh_cooldown(&self, actor: ActorId, spell: SpellId) -> Option<Cooldown> {
        let def = self.s.setup.data.spells.get(&spell)?.cooldown.as_ref()?;
        Some(self.new_cooldown(actor, spell, def))
    }

    fn new_cooldown(&self, actor: ActorId, spell: SpellId, def: &CooldownDef) -> Cooldown {
        let pct = self.mod_sum(actor, spell, ModKind::CooldownPct, None);
        let charges = def.charges.max(1);
        Cooldown {
            charges,
            max: charges,
            base: f64::from(def.duration.millis()) * (1.0 + pct / 100.0).max(0.0),
            hasted: def.hasted,
            rate_mult: 1.0,
            progress: 0.0,
            at: self.now,
            gen: 0,
        }
    }

    /// When the next charge returns, if the spell has none now.
    pub(crate) fn empty_until(&self, actor: ActorId, spell: SpellId) -> Option<SimTime> {
        let a = self.actor_ref(actor)?;
        let cd = a.cooldowns.get(&spell)?;
        if cd.charges > 0 {
            return None;
        }
        let mut cd = *cd;
        cd.settle(self.now, a.haste);
        cd.ready_at(a.haste)
    }

    pub(crate) fn schedule_cooldown(&mut self, actor: ActorId, spell: SpellId) {
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let haste = a.haste;
        let Some(cd) = a.cooldowns.get_mut(&spell) else {
            return;
        };
        cd.gen += 1;
        let gen = cd.gen;
        if let Some(t) = cd.ready_at(haste) {
            self.queue
                .push(t, Event::CooldownReady { actor, spell, gen });
        }
    }

    pub(crate) fn consume_charge(&mut self, actor: ActorId, spell: SpellId, def: &CooldownDef) {
        let now = self.now;
        let fresh = self.new_cooldown(actor, spell, def);
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let haste = a.haste;
        let cd = a.cooldowns.entry(spell).or_insert(fresh);
        cd.settle(now, haste);
        if cd.charges == cd.max {
            cd.progress = 0.0;
            cd.base = fresh.base;
        }
        cd.charges = cd.charges.saturating_sub(1);
        self.schedule_cooldown(actor, spell);
    }

    pub(crate) fn cooldown_ready(&mut self, actor: ActorId, spell: SpellId, gen: u32) {
        let now = self.now;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let haste = a.haste;
        let Some(cd) = a.cooldowns.get_mut(&spell) else {
            return;
        };
        if cd.gen != gen || cd.charges >= cd.max {
            return;
        }
        cd.settle(now, haste);
        cd.charges += 1;
        cd.progress = if cd.charges == cd.max {
            0.0
        } else {
            (cd.progress - cd.base).max(0.0)
        };
        self.schedule_cooldown(actor, spell);
    }

    pub(crate) fn adjust_cooldown(
        &mut self,
        actor: ActorId,
        spell: SpellId,
        change: CooldownChange,
    ) {
        let now = self.now;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let haste = a.haste;
        let Some(cd) = a.cooldowns.get_mut(&spell) else {
            return;
        };
        cd.settle(now, haste);
        let before = cd.charges;
        match change {
            CooldownChange::Reset => {
                cd.charges = cd.max;
                cd.progress = 0.0;
            }
            CooldownChange::AddCharge => {
                cd.charges = (cd.charges + 1).min(cd.max);
                if cd.charges == cd.max {
                    cd.progress = 0.0;
                }
            }
            CooldownChange::Reduce(d) => {
                if cd.charges < cd.max {
                    cd.progress += f64::from(d.millis());
                    while cd.progress >= cd.base && cd.charges < cd.max {
                        cd.progress -= cd.base;
                        cd.charges += 1;
                    }
                    if cd.charges == cd.max {
                        cd.progress = 0.0;
                    }
                }
            }
            CooldownChange::RateMult(m) => cd.rate_mult *= m,
        }
        let regained = before == 0 && cd.charges > 0;
        self.schedule_cooldown(actor, spell);
        if regained {
            if let Some(seat) = self.player_seat(actor) {
                self.notify(seat, WakeReason::CooldownReady(spell), false, None, now);
            }
        }
    }

    fn cost_mult(&self, actor: ActorId, spell: SpellId, target: Option<ActorId>) -> f64 {
        (1.0 + self.mod_sum(actor, spell, ModKind::CostPct, target) / 100.0).max(0.0)
    }

    /// Spend a completed cast's costs; returns what a scaling cost consumed.
    pub(crate) fn pay_costs(
        &mut self,
        actor: ActorId,
        spell: SpellId,
        def: &SpellDef,
        target: Option<ActorId>,
    ) -> Option<ResourceAmount> {
        let mult = self.cost_mult(actor, spell, target);
        let now = self.now;
        let a = self.actor_mut(actor)?;
        let haste = a.haste;
        let mut spent = None;
        for cost in &def.costs {
            let Some(r) = a.resources.iter_mut().find(|r| r.def.kind == cost.kind) else {
                continue;
            };
            r.settle(now, haste);
            let base = (cost.amount * mult).min(r.value);
            r.value -= base;
            let extra = cost.extra.min(r.value).max(0.0);
            r.value -= extra;
            if cost.scaling != SpendScaling::None {
                spent = Some(ResourceAmount {
                    kind: cost.kind,
                    amount: base + extra,
                });
            }
        }
        spent
    }

    /// The kernel's gates for one ability; mechanics' gate is added on top.
    pub(crate) fn base_readiness(&self, seat: Seat, ability: SpellId) -> (Readiness, WakeReason) {
        let now = self.now;
        let mut need = Need::new(now);
        let actor = self.seat_actor(seat);
        let spell = self.resolved(seat, ability);
        let (Some(a), Some(st), Some(def)) = (
            self.actor_ref(actor),
            self.seat_ref(seat),
            self.s.setup.data.spells.get(&spell),
        ) else {
            need.block();
            return need.finish(ability);
        };
        if !a.alive {
            need.block();
            return need.finish(ability);
        }
        match self.seg {
            Seg::Finished => need.block(),
            Seg::Travel {
                prepull_from,
                combat_starts,
                ..
            } => {
                if now < prepull_from {
                    need.block();
                } else if def.hostile {
                    need.until(combat_starts, WakeReason::CombatStart);
                }
            }
            Seg::Combat { .. } => {
                if def.targeting == Targeting::Enemy && self.live_targets().is_empty() {
                    need.block();
                }
            }
        }
        if let Some(c) = &a.casting {
            if !def.usable_while_casting {
                need.until(c.ends, WakeReason::CastEnd);
            }
        }
        if def.gcd.is_some() {
            if let Some(g) = st.gcd_end {
                need.until(g, WakeReason::GcdEnd);
            }
        }
        if let Some(t) = self.empty_until(actor, spell) {
            need.until(t, WakeReason::CooldownReady(ability));
        }
        let mult = self.cost_mult(actor, spell, a.target);
        for cost in &def.costs {
            let Some(r) = a.resources.iter().find(|r| r.def.kind == cost.kind) else {
                need.block();
                continue;
            };
            let want = cost.amount * mult;
            let have = r.value_at(now, a.haste);
            if have + 1e-9 < want {
                let rate = r.rate(a.haste);
                if rate > 0.0 && want <= r.def.max {
                    need.until(
                        now + millis_ceil((want - have) / rate * 1000.0),
                        WakeReason::ConditionMet,
                    );
                } else {
                    need.block();
                }
            }
        }
        need.finish(ability)
    }

    /// Who a cast would land on.
    pub(crate) fn resolve_target(
        &self,
        seat: Seat,
        def: &SpellDef,
        sel: TargetSel,
    ) -> Result<Option<ActorId>, IllegalChoice> {
        let me = self.seat_actor(seat);
        let pick = |pool: Vec<ActorId>, lowest: bool| -> Result<Option<ActorId>, IllegalChoice> {
            let frac = |id: &ActorId| {
                self.actor_ref(*id)
                    .map_or(0.0, |a| a.health / a.max_health.max(f64::MIN_POSITIVE))
            };
            let best = if lowest {
                pool.into_iter().min_by(|a, b| frac(a).total_cmp(&frac(b)))
            } else {
                pool.into_iter().max_by(|a, b| frac(a).total_cmp(&frac(b)))
            };
            best.map(Some).ok_or(IllegalChoice::NoValidTarget)
        };
        match def.targeting {
            Targeting::None => Ok(None),
            Targeting::SelfOnly => Ok(Some(me)),
            Targeting::Enemy => match sel {
                TargetSel::Primary => self
                    .actor_ref(me)
                    .and_then(|a| a.target)
                    .filter(|&t| self.is_live_target(t))
                    .map(Some)
                    .ok_or(IllegalChoice::NoValidTarget),
                TargetSel::Actor(id) if self.is_live_target(id) => Ok(Some(id)),
                TargetSel::Actor(id) => Err(IllegalChoice::NotATarget(id)),
                TargetSel::LowestHealth => pick(self.live_targets(), true),
                TargetSel::HighestHealth => pick(self.live_targets(), false),
                TargetSel::Casting => Err(IllegalChoice::NoValidTarget),
            },
            Targeting::Ally => {
                let allies: Vec<ActorId> = self
                    .seat_actors
                    .iter()
                    .copied()
                    .filter(|&id| self.actor_ref(id).is_some_and(|a| a.alive))
                    .collect();
                match sel {
                    TargetSel::Primary => Ok(Some(me)),
                    TargetSel::Actor(id) if allies.contains(&id) => Ok(Some(id)),
                    TargetSel::Actor(id) => Err(IllegalChoice::NotATarget(id)),
                    TargetSel::LowestHealth => pick(allies, true),
                    TargetSel::HighestHealth => pick(allies, false),
                    TargetSel::Casting => Err(IllegalChoice::NoValidTarget),
                }
            }
        }
    }
}
