//! Triggers and enemy rules: when enemies engage, when their rules fire,
//! and what the rules do.
//!
//! A rule's clock starts when its enemy engages. Its trigger is checked
//! whenever triggers are re-evaluated; once it holds, the rule fires, then
//! again every `repeat` while its phase is active. A rule whose phase ends
//! stops repeating, and resumes when the phase returns and its trigger
//! holds.

use std::sync::Arc;

use portunus_core::rng::{self, Purpose};
use portunus_core::{ActorId, Dist, EventName, Sample, Seat, SimDuration, SimTime, Trigger};
use portunus_gamedata::effect::Effect;
use portunus_gamedata::enemy::{EnemyAction, EnemyDef, EnemyTarget, RuleIndex};
use portunus_gamedata::spec::Role;
use portunus_scenario::resolved::{ResolvedCombat, Segment, SpawnSet};

use crate::mechanics::{AuraApplication, EnemyHit, Mechanics};
use crate::state::{ActorKind, AuraRef};
use crate::step::WakeReason;
use crate::trace::{CastEndReason, TraceEvent};

use super::queue::Event;
use super::world::{EnemyCast, Statics, TriggerOwner, World};
use super::Kernel;

/// Random draws per rule, by what they decide.
const TIMING: usize = 0;
const TARGETS: usize = 1;
const AMOUNTS: usize = 2;

/// Every `Elapsed` duration in a trigger, so the kernel can re-evaluate at
/// those moments.
pub(super) fn elapsed_marks(t: &Trigger<SpawnSet>, out: &mut Vec<SimDuration>) {
    match t {
        Trigger::Elapsed(d) => out.push(*d),
        Trigger::All(ts) | Trigger::Any(ts) => {
            for t in ts {
                elapsed_marks(t, out);
            }
        }
        Trigger::Delayed { after, .. } => elapsed_marks(after, out),
        _ => {}
    }
}

pub(super) fn combat_def(s: &Statics, combat: u16) -> Option<&ResolvedCombat> {
    s.setup
        .run
        .segments
        .iter()
        .filter_map(|seg| match seg {
            Segment::Combat(c) => Some(c),
            Segment::Travel { .. } => None,
        })
        .nth(usize::from(combat))
}

/// The definition behind an enemy actor.
pub(super) fn enemy_def(s: &Statics, kind: ActorKind) -> Option<&EnemyDef> {
    let ActorKind::Enemy { combat, spawn } = kind else {
        return None;
    };
    let key = &combat_def(s, combat)?
        .spawns
        .get(usize::from(spawn.0))?
        .enemy;
    s.setup.enemies.enemies.get(key)
}

impl World {
    pub(super) fn members(&self, set: &SpawnSet) -> Vec<ActorId> {
        match set {
            SpawnSet::One(i) => self
                .enemies
                .get(usize::from(i.0))
                .copied()
                .into_iter()
                .collect(),
            SpawnSet::Many(is) => is
                .iter()
                .filter_map(|i| self.enemies.get(usize::from(i.0)).copied())
                .collect(),
            SpawnSet::Engaged => self
                .enemies
                .iter()
                .copied()
                .filter(|&e| self.actor_ref(e).is_some_and(|a| a.engaged))
                .collect(),
        }
    }

    /// How often the members of `set` have fired the rule named `event`,
    /// summed.
    fn fired_count(&self, set: &SpawnSet, event: &EventName) -> u32 {
        self.members(set)
            .into_iter()
            .filter_map(|e| {
                let a = self.actor_ref(e)?;
                let def = enemy_def(&self.s, a.kind)?;
                let i = def.rules.iter().position(|r| &r.name == event)?;
                Some(a.rules.get(i)?.fired)
            })
            .sum()
    }

    /// Whether a trigger holds now. `Delayed` nodes remember, per owner,
    /// when their condition first held.
    pub(super) fn trigger_holds(
        &mut self,
        t: &Trigger<SpawnSet>,
        started: SimTime,
        owner: TriggerOwner,
    ) -> bool {
        let mut node = 0;
        self.holds(t, started, owner, &mut node)
    }

    /// Evaluates every node, without short-circuiting, so each `Delayed`
    /// node keeps the same position whatever the other nodes say.
    fn holds(
        &mut self,
        t: &Trigger<SpawnSet>,
        started: SimTime,
        owner: TriggerOwner,
        node: &mut u32,
    ) -> bool {
        let id = *node;
        *node += 1;
        let group = |w: &World, set: &SpawnSet| -> Vec<(u64, u64, bool)> {
            w.members(set)
                .into_iter()
                .filter_map(|e| w.actor_ref(e))
                .map(|a| (a.health, a.max_health, a.alive))
                .collect()
        };
        match t {
            Trigger::Now => true,
            Trigger::Never => false,
            Trigger::Elapsed(d) => self.now >= started + *d,
            Trigger::HpFracBelow { who, frac } => {
                let g = group(self, who);
                let max: u64 = g.iter().map(|m| m.1).sum();
                let hp: u64 = g.iter().map(|m| m.0).sum();
                max > 0 && (hp as f64) < *frac * max as f64
            }
            Trigger::AliveAtMost { who, count } => {
                group(self, who).iter().filter(|m| m.2).count() <= *count as usize
            }
            Trigger::Died(who) => group(self, who).iter().all(|m| !m.2),
            Trigger::Fired { who, event, nth } => self.fired_count(who, event) >= *nth,
            Trigger::Delayed { delay, after } => {
                let inner = self.holds(after, started, owner, node);
                let key = (owner, id);
                if inner && !self.delayed.contains_key(&key) {
                    self.delayed.insert(key, self.now);
                    self.queue.push(self.now + *delay, Event::EvalTriggers);
                }
                self.delayed
                    .get(&key)
                    .is_some_and(|&first| self.now >= first + *delay)
            }
            Trigger::All(ts) => {
                let each: Vec<bool> = ts
                    .iter()
                    .map(|t| self.holds(t, started, owner, node))
                    .collect();
                each.into_iter().all(|b| b)
            }
            Trigger::Any(ts) => {
                let each: Vec<bool> = ts
                    .iter()
                    .map(|t| self.holds(t, started, owner, node))
                    .collect();
                each.into_iter().any(|b| b)
            }
        }
    }

    /// Queue re-evaluations at the moments an enemy's rule triggers name.
    pub(crate) fn schedule_rule_marks(&mut self, enemy: ActorId) {
        let s = Arc::clone(&self.s);
        let Some(ActorKind::Enemy { combat, spawn }) = self.actor_ref(enemy).map(|a| a.kind) else {
            return;
        };
        let mut marks = Vec::new();
        for t in s
            .rule_triggers
            .get(usize::from(combat))
            .and_then(|c| c.get(usize::from(spawn.0)))
            .into_iter()
            .flatten()
        {
            elapsed_marks(t, &mut marks);
        }
        let now = self.now;
        for d in marks {
            self.queue.push(now + d, Event::EvalTriggers);
        }
    }

    /// The rule has no phase, or its enemy is in it.
    pub(crate) fn rule_active(&self, enemy: ActorId, rule: RuleIndex) -> bool {
        let Some(a) = self.actor_ref(enemy) else {
            return false;
        };
        enemy_def(&self.s, a.kind)
            .and_then(|d| d.rules.get(usize::from(rule.0)))
            .is_some_and(|r| r.phase.as_ref().is_none_or(|p| a.phase.as_ref() == Some(p)))
    }

    /// The next random index for one of a rule's draw streams, and its
    /// domain.
    fn rule_draw(
        &mut self,
        enemy: ActorId,
        rule: RuleIndex,
        which: usize,
        purpose: Purpose,
    ) -> (rng::Domain, u64) {
        let s = Arc::clone(&self.s);
        let Some(a) = self.actor_mut(enemy) else {
            return (rng::domain(purpose, &[]), 0);
        };
        let name = enemy_def(&s, a.kind)
            .and_then(|d| d.rules.get(usize::from(rule.0)))
            .map_or("", |r| r.name.0.as_str());
        let domain = rng::domain(purpose, &[&*a.name, name]);
        let index = a.rules.get_mut(usize::from(rule.0)).map_or(0, |st| {
            let i = st.draws[which];
            st.draws[which] += 1;
            i
        });
        (domain, index)
    }

    fn rule_duration(
        &mut self,
        enemy: ActorId,
        rule: RuleIndex,
        d: &Dist<SimDuration>,
    ) -> SimDuration {
        let (domain, index) = self.rule_draw(enemy, rule, TIMING, Purpose::EnemyRuleTiming);
        d.sample(self.s.setup.run.seed, domain, index)
    }

    fn rule_amount(&mut self, enemy: ActorId, rule: RuleIndex, d: &Dist<f64>) -> f64 {
        let (domain, index) = self.rule_draw(enemy, rule, AMOUNTS, Purpose::EnemyDamage);
        d.sample(self.s.setup.run.seed, domain, index)
    }

    /// Living seats a rule's action lands on. `Tank` is the party's first
    /// tank, or its first living seat if it has none.
    fn enemy_targets(&mut self, enemy: ActorId, rule: RuleIndex, target: EnemyTarget) -> Vec<Seat> {
        let alive: Vec<Seat> = (0..self.seats.len())
            .map(|i| Seat(i as u8))
            .filter(|&s| self.can_be_targeted(s))
            .collect();
        let is_tank = |w: &World, s: &Seat| w.s.roles.get(usize::from(s.0)) == Some(&Role::Tank);
        let tanks: Vec<Seat> = alive.iter().copied().filter(|s| is_tank(self, s)).collect();
        let others: Vec<Seat> = alive
            .iter()
            .copied()
            .filter(|s| !is_tank(self, s))
            .collect();
        match target {
            EnemyTarget::AllPlayers => alive,
            EnemyTarget::Tank => tanks
                .first()
                .or(alive.first())
                .copied()
                .into_iter()
                .collect(),
            EnemyTarget::RandomPlayer => self.pick(enemy, rule, alive, 1),
            EnemyTarget::RandomNonTank => {
                let pool = if others.is_empty() { alive } else { others };
                self.pick(enemy, rule, pool, 1)
            }
            EnemyTarget::RandomPlayers(n) => self.pick(enemy, rule, alive, usize::from(n)),
        }
    }

    /// `n` distinct seats, evenly at random.
    fn pick(
        &mut self,
        enemy: ActorId,
        rule: RuleIndex,
        mut pool: Vec<Seat>,
        n: usize,
    ) -> Vec<Seat> {
        let n = n.min(pool.len());
        for i in 0..n {
            let (domain, index) = self.rule_draw(enemy, rule, TARGETS, Purpose::EnemyTarget);
            let u = rng::unit(self.s.setup.run.seed, domain, index);
            let span = pool.len() - i;
            let j = i + ((u * span as f64) as usize).min(span - 1);
            pool.swap(i, j);
        }
        pool.truncate(n);
        pool
    }

    /// An enemy casts one thing at a time: a cast that would start while
    /// another is in progress is skipped.
    fn start_enemy_cast(
        &mut self,
        enemy: ActorId,
        rule: RuleIndex,
        time: SimDuration,
        interruptible: bool,
        then: Arc<EnemyAction>,
    ) {
        let now = self.now;
        let Some(a) = self.actor_mut(enemy) else {
            return;
        };
        if a.enemy_cast.is_some() {
            return;
        }
        a.cast_seq += 1;
        let seq = a.cast_seq;
        let ends = now + time;
        a.enemy_cast = Some(EnemyCast {
            rule,
            then,
            started: now,
            ends,
            interruptible,
            seq,
        });
        self.record(TraceEvent::EnemyCastStart {
            actor: enemy,
            rule,
            ends,
        });
        self.queue.push(ends, Event::EnemyCastEnd { enemy, seq });
        for i in 0..self.seats.len() {
            self.notify(
                Seat(i as u8),
                WakeReason::EnemyCastStart(enemy),
                false,
                None,
                now,
            );
        }
    }

    /// Returns whether a cast was in progress.
    pub(crate) fn cancel_enemy_cast(&mut self, enemy: ActorId, reason: CastEndReason) -> bool {
        let Some(c) = self.actor_mut(enemy).and_then(|a| a.enemy_cast.take()) else {
            return false;
        };
        self.record(TraceEvent::EnemyCastEnd {
            actor: enemy,
            rule: c.rule,
            reason,
        });
        true
    }

    /// Stop an enemy's interruptible cast.
    pub(crate) fn interrupt(&mut self, target: ActorId) -> bool {
        let interruptible = self
            .actor_ref(target)
            .and_then(|a| a.enemy_cast.as_ref())
            .is_some_and(|c| c.interruptible);
        interruptible && self.cancel_enemy_cast(target, CastEndReason::Interrupted)
    }
}

impl<M: Mechanics> Kernel<M> {
    /// Queue a firing for every rule whose trigger now holds.
    pub(super) fn arm_rules(&mut self) {
        let s = Arc::clone(&self.world.s);
        let Some(combat) = self.world.combat_index() else {
            return;
        };
        let Some(triggers) = s.rule_triggers.get(usize::from(combat)) else {
            return;
        };
        let now = self.world.now;
        for (spawn, enemy) in self.world.enemies.clone().into_iter().enumerate() {
            let Some(a) = self.world.actor_ref(enemy) else {
                continue;
            };
            if !(a.alive && a.engaged) {
                continue;
            }
            let started = a.engaged_at.unwrap_or(now);
            let Some(def) = enemy_def(&s, a.kind) else {
                continue;
            };
            for (r, trigger) in triggers.get(spawn).into_iter().flatten().enumerate() {
                let rule = RuleIndex(r as u16);
                let (Some(st), Some(rdef)) = (
                    self.world
                        .actor_ref(enemy)
                        .and_then(|a| a.rules.get(r))
                        .copied(),
                    def.rules.get(r),
                ) else {
                    continue;
                };
                if st.fired > 0 && rdef.repeat.is_none() {
                    continue;
                }
                let holds = self
                    .world
                    .trigger_holds(trigger, started, (enemy, Some(rule)));
                if !holds || st.next_at.is_some() || !self.world.rule_active(enemy, rule) {
                    continue;
                }
                let Some(st) = self.world.actor_mut(enemy).and_then(|a| a.rules.get_mut(r)) else {
                    continue;
                };
                st.gen += 1;
                st.next_at = Some(now);
                let gen = st.gen;
                self.world
                    .queue
                    .push(now, Event::RuleFire { enemy, rule, gen });
            }
        }
    }

    pub(super) fn fire_rule(&mut self, enemy: ActorId, rule: RuleIndex, gen: u32) {
        let s = Arc::clone(&self.world.s);
        let r = usize::from(rule.0);
        let now = self.world.now;
        let Some(a) = self.world.actor_ref(enemy) else {
            return;
        };
        if !(a.alive && a.engaged) || a.rules.get(r).is_none_or(|st| st.gen != gen) {
            return;
        }
        let Some(rdef) = enemy_def(&s, a.kind).and_then(|d| d.rules.get(r)) else {
            return;
        };
        let active = self.world.in_combat() && self.world.rule_active(enemy, rule);
        let Some(st) = self.world.actor_mut(enemy).and_then(|a| a.rules.get_mut(r)) else {
            return;
        };
        st.next_at = None;
        if !active {
            return;
        }
        st.fired += 1;
        st.last_fired = Some(now);
        if let Some(repeat) = &rdef.repeat {
            let every = self
                .world
                .rule_duration(enemy, rule, repeat)
                .max(SimDuration(1));
            if let Some(st) = self.world.actor_mut(enemy).and_then(|a| a.rules.get_mut(r)) {
                st.gen += 1;
                st.next_at = Some(now + every);
                let gen = st.gen;
                self.world
                    .queue
                    .push(now + every, Event::RuleFire { enemy, rule, gen });
            }
        }
        self.world
            .record(TraceEvent::EnemyRule { actor: enemy, rule });
        self.run_action(enemy, rule, &rdef.action);
        self.world.queue_triggers();
    }

    fn run_action(&mut self, enemy: ActorId, rule: RuleIndex, action: &EnemyAction) {
        if !self.world.actor_ref(enemy).is_some_and(|a| a.alive) {
            return;
        }
        let now = self.world.now;
        match action {
            EnemyAction::Damage {
                amount,
                school,
                target,
            } => {
                for seat in self.world.enemy_targets(enemy, rule, *target) {
                    let hit = EnemyHit {
                        source: enemy,
                        target: self.world.seat_actor(seat),
                        rule,
                        amount: self.world.rule_amount(enemy, rule, amount),
                        school: *school,
                    };
                    self.call(|m, io| m.enemy_hit(io, &hit));
                }
            }
            EnemyAction::Cast {
                time,
                interruptible,
                then,
            } => {
                let then = Arc::new((**then).clone());
                self.world
                    .start_enemy_cast(enemy, rule, *time, *interruptible, then);
            }
            EnemyAction::ForceMovement { duration, target } => {
                let until = now + self.world.rule_duration(enemy, rule, duration);
                for seat in self.world.enemy_targets(enemy, rule, *target) {
                    self.world.force_move(seat, until);
                }
            }
            EnemyAction::MustMove {
                yards,
                within,
                target,
                on_fail,
            } => {
                let on_fail: Arc<[Effect]> = on_fail.as_slice().into();
                for seat in self.world.enemy_targets(enemy, rule, *target) {
                    self.world
                        .add_demand(seat, enemy, rule, *yards, *within, Arc::clone(&on_fail));
                }
            }
            EnemyAction::Reposition { distance } => {
                let d = self.world.rule_amount(enemy, rule, distance);
                for i in 0..self.world.seats.len() {
                    self.world.set_distance(Seat(i as u8), enemy, d);
                }
            }
            EnemyAction::Knockback { yards, target } => {
                for seat in self.world.enemy_targets(enemy, rule, *target) {
                    self.world.knock_back(seat, enemy, *yards);
                }
            }
            EnemyAction::Effects { effects, target } => {
                for seat in self.world.enemy_targets(enemy, rule, *target) {
                    let t = self.world.seat_actor(seat);
                    self.call(|m, io| m.enemy_effects(io, enemy, t, effects));
                }
            }
            EnemyAction::SelfAura(aura) => {
                self.world.apply_aura(AuraApplication {
                    aura: AuraRef {
                        holder: enemy,
                        aura: *aura,
                        source: enemy,
                    },
                    stacks: 1,
                    duration: None,
                });
                self.drain();
            }
            EnemyAction::SpawnAdds { .. } => self.world.unsupported("adds"),
            EnemyAction::EnterPhase(phase) => {
                if let Some(a) = self.world.actor_mut(enemy) {
                    a.phase = Some(phase.clone());
                }
                self.world.queue_triggers();
            }
            EnemyAction::Sequence(actions) => {
                for a in actions {
                    self.run_action(enemy, rule, a);
                }
            }
        }
    }

    pub(super) fn enemy_cast_end(&mut self, enemy: ActorId, seq: u32) {
        let Some(a) = self.world.actor_mut(enemy) else {
            return;
        };
        if !a.alive || a.enemy_cast.as_ref().is_none_or(|c| c.seq != seq) {
            return;
        }
        let Some(c) = a.enemy_cast.take() else { return };
        self.world.record(TraceEvent::EnemyCastEnd {
            actor: enemy,
            rule: c.rule,
            reason: CastEndReason::Completed,
        });
        self.run_action(enemy, c.rule, &c.then);
        self.world.queue_triggers();
    }

    /// A demand's time ran out: if still owed, its enemy's failure effects
    /// land on the seat.
    pub(super) fn demand_deadline(&mut self, seat: Seat, id: u32) {
        let Some(d) = self.world.take_failed_demand(seat, id) else {
            return;
        };
        let target = self.world.seat_actor(seat);
        if !d.on_fail.is_empty() && self.world.actor_ref(target).is_some_and(|a| a.alive) {
            self.call(|m, io| m.enemy_effects(io, d.source, target, &d.on_fail));
        }
    }
}
