//! Auto-attack timers.
//!
//! An actor with a weapon swings at its target (a pet's is its owner's)
//! whenever it is in combat and that target is alive, independent of its
//! casts. A timer that finds nothing to hit stops, and starts again at
//! once when a target appears. Haste and attack speed changes rescale the
//! time left on a running swing.

use portunus_core::{ActorId, SimTime};
use portunus_gamedata::item::WeaponHand;

use crate::mechanics::{Mechanics, SwingEvent};
use crate::state::StateView;

use super::queue::Event;
use super::world::{hand_index, millis_round, World};
use super::Kernel;

const HANDS: [WeaponHand; 2] = [WeaponHand::MainHand, WeaponHand::OffHand];

impl World {
    /// The live target an actor's swings would land on.
    fn swing_target(&self, actor: ActorId) -> Option<ActorId> {
        if !self.in_combat() || !self.actor_ref(actor).is_some_and(|a| a.alive) {
            return None;
        }
        self.target(actor).filter(|&t| self.is_live_target(t))
    }

    /// Start any stopped timers if there is something to hit. The off hand
    /// opens half an interval behind the main hand.
    pub(crate) fn start_swings(&mut self, actor: ActorId) {
        if self.swing_target(actor).is_none() {
            return;
        }
        let now = self.now;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let speed = a.swing_speed();
        let mut due = Vec::new();
        for hand in HANDS {
            let Some(s) = &mut a.swings[hand_index(hand)] else {
                continue;
            };
            if s.next_at.is_some() {
                continue;
            }
            let delay = match hand {
                WeaponHand::MainHand => portunus_core::SimDuration::ZERO,
                WeaponHand::OffHand => millis_round(f64::from(s.interval(speed).millis()) / 2.0),
            };
            let at = now + delay;
            s.next_at = Some(at);
            s.gen += 1;
            due.push((
                at,
                Event::Swing {
                    actor,
                    hand,
                    gen: s.gen,
                },
            ));
        }
        for (at, ev) in due {
            self.queue.push(at, ev);
        }
    }

    pub(crate) fn stop_swings(&mut self, actor: ActorId) {
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        for s in a.swings.iter_mut().flatten() {
            s.next_at = None;
            s.gen += 1;
        }
    }

    /// After the actor's swing speed changed from `old_speed`.
    pub(crate) fn rescale_swings(&mut self, actor: ActorId, old_speed: f64) {
        let now = self.now;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        let ratio = old_speed / a.swing_speed();
        let mut due = Vec::new();
        for hand in HANDS {
            let Some(s) = &mut a.swings[hand_index(hand)] else {
                continue;
            };
            let Some(next) = s.next_at.filter(|&t| t > now) else {
                continue;
            };
            let left = f64::from(next.saturating_since(now).millis()) * ratio;
            let at = now + millis_round(left);
            s.next_at = Some(at);
            s.gen += 1;
            due.push((
                at,
                Event::Swing {
                    actor,
                    hand,
                    gen: s.gen,
                },
            ));
        }
        for (at, ev) in due {
            self.queue.push(at, ev);
        }
    }

    /// A swing is due now; returns its target, or stops the timer.
    fn take_swing(&mut self, actor: ActorId, hand: WeaponHand, gen: u32) -> Option<ActorId> {
        let now = self.now;
        let target = self.swing_target(actor);
        let s = self.actor_mut(actor)?.swings[hand_index(hand)].as_mut()?;
        if s.gen != gen || s.next_at != Some(now) {
            return None;
        }
        if target.is_none() {
            s.next_at = None;
            s.gen += 1;
        }
        target
    }

    fn next_swing(&mut self, actor: ActorId, hand: WeaponHand) {
        let now = self.now;
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        if !a.alive {
            return;
        }
        let speed = a.swing_speed();
        let Some(s) = &mut a.swings[hand_index(hand)] else {
            return;
        };
        if s.next_at != Some(now) {
            return;
        }
        let at: SimTime = now + s.interval(speed);
        s.next_at = Some(at);
        s.gen += 1;
        let gen = s.gen;
        self.queue.push(at, Event::Swing { actor, hand, gen });
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
