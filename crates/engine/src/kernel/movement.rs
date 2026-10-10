//! Movement: run speed, voluntary and forced moves, movement demands, and
//! distances to enemies.
//!
//! Space is one-dimensional. Each enemy keeps a distance per seat; moving
//! toward an enemy closes it, and any movement at all counts toward every
//! demand the seat owes, since dodging is about leaving a spot, not about
//! where you go. Progress accrues continuously and is settled into the
//! demands, the goal, and the approached enemy whenever something that
//! depends on it changes.

use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Seat, SimDuration, SimTime, SpellId};
use portunus_gamedata::effect::{Effect, ModKind};
use portunus_gamedata::enemy::RuleIndex;
use portunus_gamedata::spell::{SpellDef, Targeting};

use crate::choice::MoveGoal;
use crate::state::{ActorKind, SeatPhase};
use crate::step::WakeReason;
use crate::trace::{CastEndReason, TraceEvent};

use super::queue::Event;
use super::world::{millis_ceil, Demand, Moving, World};

/// Yards per second with no speed modifiers.
pub(crate) const BASE_RUN_SPEED: f64 = 7.0;

/// Distances closer than this count as reached.
const EPS: f64 = 1e-6;

fn secs(d: SimDuration) -> f64 {
    f64::from(d.millis()) / 1000.0
}

impl World {
    /// Yards per second.
    pub(crate) fn speed(&self, seat: Seat) -> f64 {
        let me = self.seat_actor(seat);
        let pct = self.mod_sum(me, SpellId(0), ModKind::MoveSpeedPct, None);
        BASE_RUN_SPEED * (1.0 + pct / 100.0).max(0.0)
    }

    /// Yards covered since the seat's movement was last settled.
    pub(crate) fn unsettled_yards(&self, seat: Seat) -> f64 {
        self.seat_ref(seat)
            .and_then(|st| st.moving)
            .map_or(0.0, |m| {
                self.speed(seat) * secs(self.now.saturating_since(m.at))
            })
    }

    fn approaching(&self, seat: Seat) -> Option<ActorId> {
        match self.seat_ref(seat)?.moving?.goal? {
            MoveGoal::Approach { target, .. } => Some(target),
            MoveGoal::ClearDemands | MoveGoal::Yards(_) => None,
        }
    }

    /// `None` if `enemy` isn't an enemy.
    pub(crate) fn distance_now(&self, seat: Seat, enemy: ActorId) -> Option<f64> {
        let a = self.actor_ref(enemy)?;
        if !matches!(a.kind, ActorKind::Enemy { .. }) {
            return None;
        }
        let base = a.distances.get(usize::from(seat.0)).copied().unwrap_or(0.0);
        let closing = if self.approaching(seat) == Some(enemy) {
            self.unsettled_yards(seat)
        } else {
            0.0
        };
        Some((base - closing).max(0.0))
    }

    /// Instants, spells flagged castable while moving, and spells a
    /// `CastWhileMoving` modifier covers.
    pub(crate) fn usable_while_moving(
        &self,
        actor: ActorId,
        spell: SpellId,
        def: &SpellDef,
    ) -> bool {
        def.castable_while_moving
            || self.cast_time_of(actor, spell, None) == SimDuration::ZERO
            || self.has_mod(actor, spell, ModKind::CastWhileMoving)
    }

    /// When the seat's primary target comes into `range` while it
    /// approaches; `None` if it is out of range and not getting closer.
    pub(crate) fn in_range_at(&self, seat: Seat, target: ActorId, range: f64) -> Option<SimTime> {
        let d = self.distance_now(seat, target)?;
        if d <= range + EPS {
            return Some(self.now);
        }
        let speed = self.speed(seat);
        (self.approaching(seat) == Some(target) && speed > 0.0)
            .then(|| self.now + millis_ceil((d - range) / speed * 1000.0))
    }

    /// Some enemy ability's range doesn't reach the seat's primary target.
    pub(crate) fn target_out_of_range(&self, seat: Seat) -> bool {
        let me = self.seat_actor(seat);
        let Some(t) = self
            .actor_ref(me)
            .and_then(|a| a.target)
            .filter(|&t| self.is_live_target(t))
        else {
            return false;
        };
        let Some(d) = self.distance_now(seat, t) else {
            return false;
        };
        let data = &self.s.setup.data;
        self.s.abilities[usize::from(seat.0)]
            .iter()
            .any(|&ability| {
                data.spells
                    .get(&self.resolved(seat, ability))
                    .is_some_and(|def| {
                        def.targeting == Targeting::Enemy && def.range.is_some_and(|r| d > r + EPS)
                    })
            })
    }

    /// Standing still with movement owed or a target out of reach: worth
    /// asking about even with nothing to cast.
    pub(crate) fn wants_to_move(&self, seat: Seat) -> bool {
        let Some(st) = self.seat_ref(seat) else {
            return false;
        };
        self.in_combat()
            && st.moving.is_none()
            && self
                .actor_ref(self.seat_actor(seat))
                .is_some_and(|a| a.alive)
            && (!st.demands.is_empty() || self.target_out_of_range(seat))
    }

    pub(crate) fn can_move(&self, seat: Seat) -> bool {
        self.in_combat()
            && self
                .actor_ref(self.seat_actor(seat))
                .is_some_and(|a| a.alive)
            && self
                .seat_ref(seat)
                .is_some_and(|st| st.moving.is_none_or(|m| m.forced_until.is_none()))
    }

    /// Yards left to a goal, as of the last settle.
    pub(crate) fn goal_left(&self, seat: Seat, goal: MoveGoal) -> f64 {
        match goal {
            MoveGoal::ClearDemands => self.seat_ref(seat).map_or(0.0, |st| {
                st.demands.iter().map(|d| d.yards).fold(0.0, f64::max)
            }),
            MoveGoal::Approach { target, within } => {
                if !self.is_live_target(target) {
                    return 0.0;
                }
                self.distance_now(seat, target)
                    .map_or(0.0, |d| (d - within).max(0.0))
            }
            MoveGoal::Yards(y) => y.max(0.0),
        }
    }

    /// Fold the distance covered since the last settle into the seat's
    /// demands, goal, and approached enemy.
    pub(crate) fn settle_movement(&mut self, seat: Seat) {
        let yards = self.unsettled_yards(seat);
        let now = self.now;
        let approach = self.approaching(seat);
        let st = self.seat_mut(seat);
        let Some(m) = &mut st.moving else { return };
        m.at = now;
        if let Some(MoveGoal::Yards(y)) = &mut m.goal {
            *y = (*y - yards).max(0.0);
        }
        if yards <= 0.0 {
            return;
        }
        if let Some(t) = approach {
            if let Some(d) = self
                .actor_mut(t)
                .and_then(|a| a.distances.get_mut(usize::from(seat.0)))
            {
                *d = (*d - yards).max(0.0);
            }
        }
        self.cover(seat, yards);
    }

    /// Count `yards` toward every demand the seat owes; met ones go.
    fn cover(&mut self, seat: Seat, yards: f64) {
        let st = self.seat_mut(seat);
        for d in &mut st.demands {
            d.yards = (d.yards - yards).max(0.0);
        }
        let before = st.demands.len();
        st.demands.retain(|d| d.yards > EPS);
        let met = before - st.demands.len();
        let actor = self.seat_actor(seat);
        for _ in 0..met {
            self.record(TraceEvent::DemandMet { actor });
        }
    }

    /// After a settle: when the movement ends at the current speed, or end
    /// it now if its goal is met.
    pub(crate) fn reschedule_movement(&mut self, seat: Seat) {
        let Some(m) = self.seat_ref(seat).and_then(|st| st.moving) else {
            return;
        };
        let ends = match (m.forced_until, m.goal) {
            (Some(until), _) => until,
            (None, Some(goal)) => {
                let left = self.goal_left(seat, goal);
                if left <= EPS {
                    self.finish_movement(seat);
                    return;
                }
                let speed = self.speed(seat);
                if speed <= 0.0 {
                    SimTime(u32::MAX / 2)
                } else {
                    self.now + millis_ceil(left / speed * 1000.0)
                }
            }
            (None, None) => {
                self.finish_movement(seat);
                return;
            }
        };
        let st = self.seat_mut(seat);
        st.move_gen += 1;
        let gen = st.move_gen;
        if let Some(m) = &mut st.moving {
            m.ends = ends;
        }
        if ends < SimTime(u32::MAX / 2) {
            self.queue.push(ends, Event::MovementEnd { seat, gen });
        }
        let me = self.seat_actor(seat);
        self.start_swings(me);
    }

    fn finish_movement(&mut self, seat: Seat) {
        let st = self.seat_mut(seat);
        if st.moving.take().is_none() {
            return;
        }
        st.move_gen += 1;
        let actor = self.seat_actor(seat);
        self.record(TraceEvent::MovementEnd { actor });
        let now = self.now;
        self.notify(seat, WakeReason::MovementEnd, true, None, now);
        self.start_swings(actor);
    }

    pub(crate) fn movement_end(&mut self, seat: Seat, gen: u32) {
        if self.seat_ref(seat).is_none_or(|st| st.move_gen != gen) {
            return;
        }
        self.settle_movement(seat);
        let forced_over = self
            .seat_ref(seat)
            .and_then(|st| st.moving)
            .and_then(|m| m.forced_until)
            .is_some_and(|until| self.now >= until);
        if forced_over {
            self.finish_movement(seat);
        } else {
            self.reschedule_movement(seat);
        }
    }

    /// Stop a cast that can't go on while moving. Returns whether one
    /// stopped.
    fn stop_cast_for_movement(&mut self, seat: Seat, reason: CastEndReason) -> bool {
        let me = self.seat_actor(seat);
        let Some(c) = self.actor_ref(me).and_then(|a| a.casting) else {
            return false;
        };
        let Some(def) = self.s.setup.data.spells.get(&c.ev.spell) else {
            return false;
        };
        if self.usable_while_moving(me, c.ev.spell, def) {
            return false;
        }
        self.stop_cast(me, reason);
        true
    }

    pub(crate) fn start_move(&mut self, seat: Seat, goal: MoveGoal) {
        self.settle_movement(seat);
        self.stop_cast_for_movement(seat, CastEndReason::Stopped);
        let now = self.now;
        let started = self
            .seat_ref(seat)
            .and_then(|st| st.moving)
            .map_or(now, |m| m.started);
        self.seat_mut(seat).moving = Some(Moving {
            started,
            at: now,
            goal: Some(goal),
            forced_until: None,
            ends: now,
        });
        self.reschedule_movement(seat);
        if let Some(m) = self.seat_ref(seat).and_then(|st| st.moving) {
            let actor = self.seat_actor(seat);
            self.record(TraceEvent::MovementStart {
                actor,
                ends: m.ends,
                forced: false,
            });
        }
    }

    pub(crate) fn stop_move(&mut self, seat: Seat) {
        self.settle_movement(seat);
        let st = self.seat_mut(seat);
        if st.moving.take().is_some() {
            st.move_gen += 1;
            let actor = self.seat_actor(seat);
            self.record(TraceEvent::MovementEnd { actor });
        }
    }

    /// The seat must keep moving until `until`; a hard cast that can't
    /// continue stops.
    pub(crate) fn force_move(&mut self, seat: Seat, until: SimTime) {
        if !self.can_be_targeted(seat) {
            return;
        }
        self.settle_movement(seat);
        if self.stop_cast_for_movement(seat, CastEndReason::Interrupted) {
            let st = self.seat_mut(seat);
            if st.phase == SeatPhase::Committed {
                st.phase = SeatPhase::Locked;
            }
        }
        let now = self.now;
        let st = self.seat_mut(seat);
        let (started, until) = match st.moving {
            Some(m) => (m.started, m.forced_until.map_or(until, |u| u.max(until))),
            None => (now, until),
        };
        st.moving = Some(Moving {
            started,
            at: now,
            goal: None,
            forced_until: Some(until),
            ends: until,
        });
        self.reschedule_movement(seat);
        let actor = self.seat_actor(seat);
        self.record(TraceEvent::MovementStart {
            actor,
            ends: until,
            forced: true,
        });
        self.notify(seat, WakeReason::MovementStart, false, None, now);
    }

    /// Alive and in the fight: enemy rules can reach it.
    pub(crate) fn can_be_targeted(&self, seat: Seat) -> bool {
        self.seat_ref(seat)
            .is_some_and(|st| !matches!(st.phase, SeatPhase::Idle | SeatPhase::Dead))
            && self
                .actor_ref(self.seat_actor(seat))
                .is_some_and(|a| a.alive)
    }

    pub(crate) fn clear_movement(&mut self, seat: Seat) {
        let st = self.seat_mut(seat);
        st.moving = None;
        st.demands.clear();
        st.move_gen += 1;
    }

    pub(crate) fn add_demand(
        &mut self,
        seat: Seat,
        source: ActorId,
        rule: RuleIndex,
        yards: f64,
        within: SimDuration,
        on_fail: Arc<[Effect]>,
    ) {
        if !self.can_be_targeted(seat) {
            return;
        }
        self.settle_movement(seat);
        let id = self.fresh_id();
        let now = self.now;
        let deadline = now + within;
        let st = self.seat_mut(seat);
        let at = st.demands.partition_point(|d| d.deadline <= deadline);
        st.demands.insert(
            at,
            Demand {
                id,
                source,
                rule,
                yards,
                placed: now,
                deadline,
                on_fail,
            },
        );
        let actor = self.seat_actor(seat);
        self.record(TraceEvent::Demand {
            actor,
            yards,
            deadline,
        });
        self.queue
            .push(deadline, Event::DemandDeadline { seat, id });
        self.reschedule_movement(seat);
        self.notify(seat, WakeReason::MustMove, false, None, now);
    }

    /// Remove a demand whose time is up, if it is still owed.
    pub(crate) fn take_failed_demand(&mut self, seat: Seat, id: u32) -> Option<Demand> {
        self.settle_movement(seat);
        let st = self.seat_mut(seat);
        let i = st.demands.iter().position(|d| d.id == id)?;
        let d = st.demands.remove(i);
        st.outcome.demands_failed += 1;
        let actor = self.seat_actor(seat);
        self.record(TraceEvent::DemandFailed { actor });
        self.reschedule_movement(seat);
        Some(d)
    }

    fn put_distance(&mut self, seat: Seat, enemy: ActorId, distance: f64) {
        if let Some(d) = self
            .actor_mut(enemy)
            .and_then(|a| a.distances.get_mut(usize::from(seat.0)))
        {
            *d = distance.max(0.0);
        }
        self.record(TraceEvent::Distance {
            actor: enemy,
            seat,
            distance: distance.max(0.0),
        });
    }

    /// The enemy moved to `distance` yards from the seat.
    pub(crate) fn set_distance(&mut self, seat: Seat, enemy: ActorId, distance: f64) {
        self.settle_movement(seat);
        self.put_distance(seat, enemy, distance);
        self.reschedule_movement(seat);
        let now = self.now;
        self.notify(seat, WakeReason::EnemyMoved(enemy), false, None, now);
    }

    /// The seat is thrown `yards` away from the enemy; it counts toward
    /// what the seat owes.
    pub(crate) fn knock_back(&mut self, seat: Seat, enemy: ActorId, yards: f64) {
        if !self.can_be_targeted(seat) {
            return;
        }
        self.settle_movement(seat);
        let d = self.distance_now(seat, enemy).unwrap_or(0.0);
        self.put_distance(seat, enemy, d + yards);
        self.cover(seat, yards);
        self.reschedule_movement(seat);
        let now = self.now;
        self.notify(seat, WakeReason::EnemyMoved(enemy), false, None, now);
    }

    /// A blink or a leap by the seat's own actor.
    pub(crate) fn displace(&mut self, actor: ActorId, yards: f64, toward: Option<ActorId>) {
        let Some(seat) = self.player_seat(actor) else {
            return;
        };
        if !(yards > 0.0 && yards.is_finite()) {
            return;
        }
        self.settle_movement(seat);
        if let Some(t) = toward {
            if let Some(d) = self.distance_now(seat, t) {
                self.put_distance(seat, t, d - yards);
            }
        }
        self.cover(seat, yards);
        self.reschedule_movement(seat);
    }

    /// Seats approaching a dead enemy stop.
    pub(crate) fn approach_lost(&mut self, enemy: ActorId) {
        for i in 0..self.seats.len() {
            let seat = Seat(i as u8);
            if self.approaching(seat) == Some(enemy) {
                self.settle_movement(seat);
                self.reschedule_movement(seat);
            }
        }
    }

    /// Whether an aura changes run speed or what can be cast on the move.
    pub(crate) fn affects_movement(&self, aura: AuraId) -> bool {
        self.s.setup.data.auras.get(&aura).is_some_and(|d| {
            d.modifiers
                .iter()
                .any(|m| matches!(m.kind, ModKind::MoveSpeedPct | ModKind::CastWhileMoving))
        })
    }

    /// Before such an aura changes on a moving seat: settle at the old
    /// speed.
    pub(crate) fn before_movement_aura(&mut self, holder: ActorId) {
        if let Some(seat) = self.player_seat(holder) {
            self.settle_movement(seat);
        }
    }

    /// After: re-time the movement, and stop a cast that can no longer go
    /// on.
    pub(crate) fn after_movement_aura(&mut self, holder: ActorId) {
        let Some(seat) = self.player_seat(holder) else {
            return;
        };
        if self.seat_ref(seat).is_none_or(|st| st.moving.is_none()) {
            return;
        }
        if self.stop_cast_for_movement(seat, CastEndReason::Interrupted) {
            let st = self.seat_mut(seat);
            if st.phase == SeatPhase::Committed {
                st.phase = SeatPhase::Locked;
            }
            let now = self.now;
            self.notify(seat, WakeReason::MovementStart, true, None, now);
        }
        self.reschedule_movement(seat);
    }
}
