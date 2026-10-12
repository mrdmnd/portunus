//! Battle resurrections: the group's shared charges, offers to dead seats,
//! and bringing a seat back in the middle of a combat.

use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Seat, SimDuration, SimTime};
use portunus_gamedata::spell::Targeting;

use crate::mechanics::AuraApplication;
use crate::state::{AuraRef, SeatPhase};
use crate::step::WakeReason;
use crate::trace::TraceEvent;

use super::queue::Event;
use super::world::World;

/// The group's battle resurrection charges as last settled; charges that
/// came due since are counted on demand.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResPool {
    pub charges: u8,
    /// When the next charge comes; `None` while full or never recharging.
    pub next_at: Option<SimTime>,
}

/// A resurrection a dead seat has been offered and will accept after its
/// reaction time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ResOffer {
    Spell {
        health_pct: f64,
    },
    /// From an aura it holds (Soulstone), which the resurrection uses up.
    Aura(AuraId),
}

impl ResPool {
    pub(crate) fn new(rules: crate::setup::BattleRes) -> Self {
        let charges = rules.initial.min(rules.max);
        ResPool {
            charges,
            next_at: (charges < rules.max && rules.every > SimDuration::ZERO)
                .then_some(SimTime::ZERO + rules.every),
        }
    }
}

impl World {
    /// Charges now, and when the next one comes.
    pub(crate) fn battle_res_now(&self) -> (u8, Option<SimTime>) {
        let rules = self.s.group.battle_res;
        let ResPool {
            mut charges,
            mut next_at,
        } = self.battle_res;
        while let Some(t) = next_at.filter(|&t| t <= self.now) {
            charges += 1;
            next_at = (charges < rules.max).then_some(t + rules.every);
        }
        (charges, next_at)
    }

    /// Takes a charge if there is one. Recharging starts again from now if
    /// the pool was full.
    fn spend_battle_res(&mut self) -> bool {
        let rules = self.s.group.battle_res;
        let (charges, next_at) = self.battle_res_now();
        if charges == 0 {
            return false;
        }
        self.battle_res = ResPool {
            charges: charges - 1,
            next_at: next_at
                .or((rules.every > SimDuration::ZERO).then_some(self.now + rules.every)),
        };
        true
    }

    /// Dead seats no resurrection is on its way to, in seat order.
    pub(crate) fn awaiting_res(&self) -> Vec<ActorId> {
        self.seat_actors
            .iter()
            .zip(&self.seats)
            .filter(|(&id, st)| {
                st.res_offer.is_none() && self.actor_ref(id).is_some_and(|a| !a.alive)
            })
            .map(|(&id, _)| id)
            .collect()
    }

    /// Offers `target`, if it is a dead seat without an offer, a
    /// resurrection it accepts after its reaction time.
    pub(crate) fn offer_res(&mut self, target: ActorId, offer: ResOffer) {
        let Some(seat) = self.player_seat(target) else {
            return;
        };
        let dead = self.actor_ref(target).is_some_and(|a| !a.alive);
        let st = self.seat_mut(seat);
        if !dead || st.res_offer.is_some() {
            return;
        }
        st.res_offer = Some(offer);
        let gen = st.gen;
        let at = self.now + self.latency_draw(seat, 1);
        self.queue.push(at, Event::AcceptRes { seat, gen });
    }

    /// A seat just died: its own resurrection aura offers itself one, or
    /// else the seats that can resurrect hear of it.
    pub(crate) fn seat_died(&mut self, seat: Seat) {
        let me = self.seat_actor(seat);
        let s = Arc::clone(&self.s);
        let own = self.actor_ref(me).and_then(|a| {
            a.auras.iter().map(|i| i.aura).find(|aura| {
                s.setup
                    .data
                    .auras
                    .get(aura)
                    .is_some_and(|d| d.self_resurrect.is_some())
            })
        });
        match own {
            Some(aura) => self.offer_res(me, ResOffer::Aura(aura)),
            None => self.tell_resurrectors(me),
        }
    }

    /// Wakes every living seat with an ability that targets dead allies.
    fn tell_resurrectors(&mut self, dead: ActorId) {
        let s = Arc::clone(&self.s);
        let now = self.now;
        for (i, abilities) in s.abilities.iter().enumerate() {
            let resurrects = abilities.iter().any(|ab| {
                s.setup
                    .data
                    .spells
                    .get(ab)
                    .is_some_and(|d| d.targeting == Targeting::DeadAlly)
            });
            if resurrects {
                self.notify(Seat(i as u8), WakeReason::AllyDied(dead), false, None, now);
            }
        }
    }

    /// The seat takes up its offer, if it still stands. In combat that
    /// costs a charge: without one a spell's offer lapses and an aura's
    /// waits for the next charge.
    pub(crate) fn accept_res(&mut self, seat: Seat, gen: u32) {
        let me = self.seat_actor(seat);
        let Some(st) = self.seat_ref(seat) else {
            return;
        };
        if st.gen != gen || st.phase != SeatPhase::Dead {
            return;
        }
        let Some(offer) = st.res_offer else { return };
        let health_pct = match offer {
            ResOffer::Spell { health_pct } => Some(health_pct),
            ResOffer::Aura(aura) => self
                .has_aura(me, aura)
                .then(|| self.s.setup.data.auras.get(&aura))
                .flatten()
                .and_then(|d| d.self_resurrect),
        };
        let Some(health_pct) = health_pct else {
            self.lapse_res(seat);
            return;
        };
        if self.in_combat() && !self.spend_battle_res() {
            match (offer, self.battle_res_now().1) {
                (ResOffer::Aura(_), Some(at)) => {
                    self.queue.push(at, Event::AcceptRes { seat, gen })
                }
                _ => self.lapse_res(seat),
            }
            return;
        }
        if let ResOffer::Aura(aura) = offer {
            self.remove_aura(me, aura, None);
        }
        self.revive(seat, health_pct);
    }

    fn lapse_res(&mut self, seat: Seat) {
        self.seat_mut(seat).res_offer = None;
        let me = self.seat_actor(seat);
        self.tell_resurrectors(me);
    }

    /// Brings a dead seat back with `health_pct` of its maximum health and
    /// its passive and party auras. In combat it fights on: its target was
    /// kept up to date while it lay dead, and it is asked again as its
    /// abilities are armed like any locked seat's. Auras granted as a pull
    /// started don't come back.
    pub(crate) fn revive(&mut self, seat: Seat, health_pct: f64) {
        let me = self.seat_actor(seat);
        let Some(a) = self.actor_mut(me) else { return };
        a.alive = true;
        let max = a.max_health;
        a.health = ((max as f64 * health_pct).round() as u64).clamp(1, max.max(1));
        let in_combat = self.in_combat();
        let st = self.seat_mut(seat);
        st.res_offer = None;
        st.gen += 1;
        st.phase = if in_combat {
            SeatPhase::Locked
        } else {
            SeatPhase::Idle
        };
        self.restore_passives(seat);
        if in_combat {
            self.start_swings(me);
        }
    }

    /// Records the seat's resurrection and gives it back its passive and
    /// party auras.
    pub(crate) fn restore_passives(&mut self, seat: Seat) {
        let me = self.seat_actor(seat);
        let s = Arc::clone(&self.s);
        self.record(TraceEvent::Resurrect { actor: me });
        let auras = s.setup.seats[usize::from(seat.0)]
            .template
            .passive_auras
            .iter()
            .chain(&s.group.party_auras);
        for &aura in auras {
            self.apply_aura(AuraApplication {
                aura: AuraRef {
                    holder: me,
                    aura,
                    source: me,
                },
                stacks: 1,
                duration: None,
                pmultiplier: None,
                anchor: None,
            });
        }
    }
}
