//! Auto-attack timers.
//!
//! An actor with a weapon swings at its target (a pet's is its owner's)
//! whenever it is in combat and that target is alive, independent of its
//! casts. A timer that finds nothing to hit stops, and starts again at
//! once when a target appears. Haste and attack speed changes rescale the
//! time left on a running swing.
//!
//! A player's swings reach [`MELEE_RANGE`] (a ranged weapon's,
//! [`AUTO_SHOT_RANGE`]): a swing due out of reach waits for the seat's
//! approach to bring it in, or stops until the distance changes. A
//! player's swing due during a hard cast waits for the cast to end or
//! stop; pets keep swinging through theirs.

use portunus_core::{ActorId, SimTime};
use portunus_gamedata::item::{WeaponDef, WeaponHand};
use portunus_gamedata::spec::MELEE_RANGE;

use crate::mechanics::{Mechanics, SwingEvent};
use crate::state::StateView;

use super::queue::Event;
use super::world::{hand_index, millis_round, World};
use super::Kernel;

const HANDS: [WeaponHand; 2] = [WeaponHand::MainHand, WeaponHand::OffHand];

/// How far a ranged weapon's auto-attacks reach, in yards.
pub(crate) const AUTO_SHOT_RANGE: f64 = 40.0;

impl World {
    /// The live target an actor's swings would land on. None while
    /// stealthed: attacking would break it.
    fn swing_target(&self, actor: ActorId) -> Option<ActorId> {
        if !self.in_combat()
            || !self.actor_ref(actor).is_some_and(|a| a.alive)
            || self.stealthed(actor)
        {
            return None;
        }
        self.target(actor).filter(|&t| self.is_live_target(t))
    }

    /// When a swing with `weapon` can next reach `target`: now if in reach,
    /// when the seat's approach brings it in, or `None`. Pets always reach.
    fn reach_at(&self, actor: ActorId, target: ActorId, weapon: &WeaponDef) -> Option<SimTime> {
        let Some(seat) = self.player_seat(actor) else {
            return Some(self.now);
        };
        let reach = if weapon.ranged {
            AUTO_SHOT_RANGE
        } else {
            MELEE_RANGE
        };
        self.in_range_at(seat, target, reach)
    }

    /// When a player's hard cast ends, if one is in progress.
    fn casting_until(&self, actor: ActorId) -> Option<SimTime> {
        self.player_seat(actor)?;
        self.actor_ref(actor)?
            .casting
            .map(|c| c.ends)
            .filter(|&ends| ends > self.now)
    }

    /// Start any stopped timers if there is something to hit. The off hand
    /// opens half an interval behind the main hand.
    pub(crate) fn start_swings(&mut self, actor: ActorId) {
        let Some(target) = self.swing_target(actor) else {
            return;
        };
        let now = self.now;
        let Some(a) = self.actor_ref(actor) else {
            return;
        };
        let speed = a.swing_speed();
        let mut due = Vec::new();
        for hand in HANDS {
            let Some(s) = &a.swings[hand_index(hand)] else {
                continue;
            };
            if s.next_at.is_some() {
                continue;
            }
            let Some(reach) = self.reach_at(actor, target, &s.weapon) else {
                continue;
            };
            let delay = match hand {
                WeaponHand::MainHand => portunus_core::SimDuration::ZERO,
                WeaponHand::OffHand => millis_round(f64::from(s.interval(speed).millis()) / 2.0),
            };
            due.push((hand, (now + delay).max(reach)));
        }
        for (hand, at) in due {
            self.put_swing(actor, hand, Some(at));
        }
    }

    pub(crate) fn stop_swings(&mut self, actor: ActorId) {
        for hand in HANDS {
            self.put_swing(actor, hand, None);
        }
    }

    /// Swings put off by a cast that has now ended early go at once.
    pub(crate) fn resume_swings(&mut self, actor: ActorId) {
        let now = self.now;
        let Some(a) = self.actor_ref(actor) else {
            return;
        };
        let paused: Vec<WeaponHand> = HANDS
            .into_iter()
            .filter(|&h| {
                a.swings[hand_index(h)]
                    .as_ref()
                    .is_some_and(|s| s.paused && s.next_at.is_some_and(|t| t > now))
            })
            .collect();
        for hand in paused {
            self.put_swing(actor, hand, Some(now));
        }
    }

    /// Set (or with `None`, stop) a timer's next swing, invalidating any
    /// swing already queued for it.
    pub(super) fn put_swing(&mut self, actor: ActorId, hand: WeaponHand, at: Option<SimTime>) {
        let Some(s) = self
            .actor_mut(actor)
            .and_then(|a| a.swings[hand_index(hand)].as_mut())
        else {
            return;
        };
        if s.next_at.is_none() && at.is_none() {
            return;
        }
        s.next_at = at;
        s.paused = false;
        s.gen += 1;
        let gen = s.gen;
        if let Some(at) = at {
            self.queue.push(at, Event::Swing { actor, hand, gen });
        }
    }

    /// After the actor's swing speed changed from `old_speed`.
    pub(crate) fn rescale_swings(&mut self, actor: ActorId, old_speed: f64) {
        let now = self.now;
        let Some(a) = self.actor_ref(actor) else {
            return;
        };
        let ratio = old_speed / a.swing_speed();
        let mut due = Vec::new();
        for hand in HANDS {
            let Some(s) = &a.swings[hand_index(hand)] else {
                continue;
            };
            if s.paused {
                continue;
            }
            let Some(next) = s.next_at.filter(|&t| t > now) else {
                continue;
            };
            let left = f64::from(next.saturating_since(now).millis()) * ratio;
            due.push((hand, now + millis_round(left)));
        }
        for (hand, at) in due {
            self.put_swing(actor, hand, Some(at));
        }
    }

    /// A swing is due now; returns its target, or puts it off (a hard
    /// cast, out of reach) or stops the timer.
    fn take_swing(&mut self, actor: ActorId, hand: WeaponHand, gen: u32) -> Option<ActorId> {
        let now = self.now;
        let s = self.actor_ref(actor)?.swings[hand_index(hand)].as_ref()?;
        if s.gen != gen || s.next_at != Some(now) {
            return None;
        }
        let weapon = s.weapon;
        let Some(target) = self.swing_target(actor) else {
            self.put_swing(actor, hand, None);
            return None;
        };
        if let Some(ends) = self.casting_until(actor) {
            self.put_swing(actor, hand, Some(ends));
            if let Some(s) = self
                .actor_mut(actor)
                .and_then(|a| a.swings[hand_index(hand)].as_mut())
            {
                s.paused = true;
            }
            return None;
        }
        match self.reach_at(actor, target, &weapon) {
            Some(at) if at <= now => Some(target),
            later => {
                self.put_swing(actor, hand, later);
                None
            }
        }
    }

    fn next_swing(&mut self, actor: ActorId, hand: WeaponHand) {
        let now = self.now;
        let Some(a) = self.actor_ref(actor) else {
            return;
        };
        if !a.alive {
            return;
        }
        let speed = a.swing_speed();
        let Some(s) = &a.swings[hand_index(hand)] else {
            return;
        };
        if s.next_at != Some(now) {
            return;
        }
        let at = now + s.interval(speed);
        self.put_swing(actor, hand, Some(at));
    }
}

impl<M: Mechanics> Kernel<M> {
    pub(super) fn swing(&mut self, actor: ActorId, hand: WeaponHand, gen: u32) {
        let Some(target) = self.world.take_swing(actor, hand, gen) else {
            return;
        };
        let ev = SwingEvent {
            actor,
            hand,
            target,
        };
        self.call(|m, io| m.swing(io, &ev));
        self.world.next_swing(actor, hand);
    }
}
