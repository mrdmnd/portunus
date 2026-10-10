//! Pets, guardians, and totems: summoning, lifetimes, commands, and the
//! autocast loop that stands in for their decisions.
//!
//! A pet acts whenever it is idle in combat: it casts the first spell in its
//! autocast list that is ready, at its owner's target. When nothing is ready
//! it sleeps until the earliest moment something could be, or until the
//! world changes under it (a new target, a returned cooldown, a haste
//! change). Melee runs alongside on its own swing timer. Owners steer pets
//! only through their own spells (`Effect::CommandPet`), as players do in
//! practice; nothing on a pet's bar is pressed by hand.

use std::sync::Arc;

use portunus_core::{ActorId, PetId, Seat, SimDuration, SpellId};
use portunus_gamedata::pet::PetKind;

use crate::choice::CastOpts;
use crate::mask::Readiness;
use crate::mechanics::{
    whole_points, AuraApplication, AuraRemoval, CastEvent, Mechanics, PetEvent,
};
use crate::state::{ActorKind, AuraRef};
use crate::trace::{CastEndReason, TraceEvent};

use super::queue::Event;
use super::world::{Actor, Followup, PetLife, Resource, Swing, World};
use super::Kernel;

impl World {
    pub(crate) fn is_pet(&self, actor: ActorId) -> bool {
        self.actor_ref(actor).is_some_and(|a| a.pet.is_some())
    }

    /// Every live pet, seat by seat, oldest first.
    pub(crate) fn all_pets(&self) -> Vec<ActorId> {
        self.pets.iter().flatten().copied().collect()
    }

    /// The owner's live pets of one type (`None`: every type).
    fn pets_of(&self, owner: Seat, pet: Option<PetId>) -> Vec<ActorId> {
        self.pets
            .get(usize::from(owner.0))
            .into_iter()
            .flatten()
            .copied()
            .filter(|&id| {
                pet.is_none_or(|want| {
                    matches!(
                        self.actor_ref(id).map(|a| a.kind),
                        Some(ActorKind::Pet { pet, .. }) if pet == want
                    )
                })
            })
            .collect()
    }

    /// Queue an autocast check, superseding any pending one. A no-op for
    /// anything but a live pet.
    pub(crate) fn schedule_pet_act(&mut self, actor: ActorId, at: portunus_core::SimTime) {
        let at = at.max(self.now);
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        if !a.alive {
            return;
        }
        let Some(life) = &mut a.pet else { return };
        life.gen += 1;
        let gen = life.gen;
        self.queue.push(at, Event::PetAct { actor, gen });
    }

    /// Something changed that may let a pet act: check its autocast now and
    /// start its swings if it has something to hit.
    pub(crate) fn wake_pet(&mut self, actor: ActorId) {
        let now = self.now;
        self.schedule_pet_act(actor, now);
        self.start_swings(actor);
    }

    pub(crate) fn summon(
        &mut self,
        owner: Seat,
        pet: PetId,
        count: u8,
        duration: Option<SimDuration>,
    ) -> Vec<ActorId> {
        let s = Arc::clone(&self.s);
        let Some(def) = s.setup.data.pets.get(&pet) else {
            self.unsupported("summoning a pet missing from the game data");
            return Vec::new();
        };
        let spins = def.autocast.iter().any(|sp| {
            s.setup.data.spells.get(sp).is_some_and(|d| {
                d.gcd.is_none()
                    && d.cooldown.is_none()
                    && matches!(d.cast, portunus_gamedata::spell::CastKind::Instant)
            })
        });
        if spins {
            self.unsupported("pet autocast spells with no GCD, cooldown, or cast time");
            return Vec::new();
        }
        let me = self.seat_actor(owner);
        let Some((owner_health, haste, owner_name)) = self
            .actor_ref(me)
            .filter(|a| a.alive)
            .map(|a| (a.max_health, a.haste, Arc::clone(&a.name)))
        else {
            return Vec::new();
        };
        let count = if def.kind.is_temporary() { count } else { 1 };
        if def.kind == PetKind::Pet {
            let controlled: Vec<ActorId> = self
                .pets_of(owner, None)
                .into_iter()
                .filter(|&id| {
                    self.actor_ref(id)
                        .and_then(|a| a.pet)
                        .is_some_and(|p| p.kind == PetKind::Pet)
                })
                .collect();
            for id in controlled {
                self.release_pet(id, false);
            }
        }
        let expires = def
            .kind
            .is_temporary()
            .then(|| duration.or(def.duration))
            .flatten()
            .map(|d| self.now + d);
        let mut out = Vec::new();
        for _ in 0..count {
            if let Some(max) = def.max_active {
                let mine = self.pets_of(owner, Some(pet));
                let excess = (mine.len() + 1).saturating_sub(usize::from(max.max(1)));
                for &old in mine.iter().take(excess) {
                    self.release_pet(old, false);
                }
            }
            let Ok(raw) = u16::try_from(self.actors.len()) else {
                self.unsupported("more than 65535 actors in one run");
                break;
            };
            let id = ActorId(raw);
            let st = self.seat_mut(owner);
            st.summons += 1;
            let serial = st.summons;
            let name = format!("{owner_name}/{}#{serial}", def.name);
            let mut actor = Actor::new(
                ActorKind::Pet { owner, pet },
                Arc::from(name.as_str()),
                whole_points(owner_health as f64 * def.scaling.health),
                haste,
            );
            actor.resources = def
                .resources
                .iter()
                .map(|&r| Resource {
                    def: r,
                    value: r.initial,
                    at: self.now,
                    regen_mult: 1.0,
                })
                .collect();
            actor.pet = Some(PetLife {
                kind: def.kind,
                expires,
                gcd_end: None,
                gen: 0,
            });
            if def.kind != PetKind::Totem {
                actor.swings[0] = def.melee.map(Swing::new);
            }
            self.actors.push(actor);
            self.pets[usize::from(owner.0)].push(id);
            self.record(TraceEvent::PetSummoned {
                owner,
                pet,
                actor: id,
            });
            for &aura in &def.passive_auras {
                self.apply_aura(AuraApplication {
                    aura: AuraRef {
                        holder: id,
                        aura,
                        source: id,
                    },
                    stacks: 1,
                    duration: None,
                });
            }
            if let Some(e) = expires {
                self.queue.push(e, Event::PetExpire { actor: id });
            }
            self.wake_pet(id);
            out.push(id);
        }
        out
    }

    pub(crate) fn dismiss(&mut self, owner: Seat, pet: PetId, count: Option<u8>) {
        let mine = self.pets_of(owner, Some(pet));
        let n = count.map_or(mine.len(), usize::from);
        for id in mine.into_iter().take(n) {
            self.release_pet(id, false);
        }
    }

    pub(crate) fn extend_pets(&mut self, owner: Seat, pet: Option<PetId>, by: SimDuration) {
        for id in self.pets_of(owner, pet) {
            let Some(life) = self.actor_mut(id).and_then(|a| a.pet.as_mut()) else {
                continue;
            };
            if let Some(e) = &mut life.expires {
                *e += by;
                let at = *e;
                self.queue.push(at, Event::PetExpire { actor: id });
            }
        }
    }

    pub(crate) fn command_pets(
        &mut self,
        owner: Seat,
        pet: Option<PetId>,
        spell: SpellId,
        target: ActorId,
    ) {
        let now = self.now;
        for caster in self.pets_of(owner, pet) {
            self.queue.push(
                now,
                Event::Triggered {
                    caster,
                    spell,
                    target: Some(target),
                    rolled: None,
                },
            );
        }
    }

    pub(crate) fn pet_expire(&mut self, actor: ActorId) {
        let now = self.now;
        let due = self
            .actor_ref(actor)
            .filter(|a| a.alive && a.pet.is_some_and(|p| p.expires == Some(now)))
            .map(|a| a.kind);
        if let Some(ActorKind::Pet { owner, pet }) = due {
            self.release_pet(actor, true);
            self.followups
                .push_back(Followup::PetExpired(PetEvent { owner, pet, actor }));
        }
    }

    /// A pet leaves without dying: timed out, dismissed, or replaced. It
    /// stops acting at once, but keeps its auras until its spells in flight
    /// land and the spells it was triggered to cast resolve, so they land
    /// as cast (SimC rolls them at execute). A pet timing out also waits
    /// out its own reaction to leaving (`expiring`).
    fn release_pet(&mut self, actor: ActorId, expiring: bool) {
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        if !a.alive {
            return;
        }
        a.alive = false;
        if let ActorKind::Pet { owner, .. } = a.kind {
            if let Some(list) = self.pets.get_mut(usize::from(owner.0)) {
                list.retain(|&p| p != actor);
            }
        }
        self.cancel_cast(actor, CastEndReason::Interrupted);
        self.record(TraceEvent::PetExpired { actor });
        if expiring || self.has_unfinished_casts(actor) {
            self.departing.push(actor);
        } else {
            self.strip_pet(actor);
        }
    }

    fn has_unfinished_casts(&self, actor: ActorId) -> bool {
        self.flights.iter().any(|(_, ev, _)| ev.actor == actor)
            || self
                .queue
                .any(|e| matches!(e, Event::Triggered { caster, .. } if *caster == actor))
    }

    /// Whether a pet that has left still finishes casting what it was
    /// triggered to.
    pub(crate) fn is_departing(&self, actor: ActorId) -> bool {
        self.departing.contains(&actor)
    }

    /// Finish the departures whose last spell has landed or been dropped.
    pub(crate) fn settle_departures(&mut self) {
        if self.departing.is_empty() {
            return;
        }
        let (waiting, done): (Vec<ActorId>, Vec<ActorId>) = std::mem::take(&mut self.departing)
            .into_iter()
            .partition(|&p| self.has_unfinished_casts(p));
        self.departing = waiting;
        for pet in done {
            self.strip_pet(pet);
        }
    }

    fn strip_pet(&mut self, actor: ActorId) {
        while let Some(last) = self
            .actor_ref(actor)
            .and_then(|a| a.auras.len().checked_sub(1))
        {
            self.remove_instance(actor, last, AuraRemoval::HolderDied);
        }
        self.forget_pet(actor);
    }

    /// Drop a pet that is gone from its owner's list, along with the auras
    /// it put on others (its DoTs end with it, as in SimC).
    pub(crate) fn forget_pet(&mut self, actor: ActorId) {
        let Some(a) = self.actor_mut(actor) else {
            return;
        };
        if let Some(life) = &mut a.pet {
            life.gen += 1;
        }
        if let ActorKind::Pet { owner, .. } = a.kind {
            if let Some(list) = self.pets.get_mut(usize::from(owner.0)) {
                list.retain(|&p| p != actor);
            }
        }
        for i in 0..self.actors.len() {
            let holder = ActorId(i as u16);
            while let Some(j) = self.actors[i].auras.iter().rposition(|x| x.source == actor) {
                self.remove_instance(holder, j, AuraRemoval::Removed);
            }
        }
    }
}

impl<M: Mechanics> Kernel<M> {
    /// Cast the first ready autocast spell, or sleep until one could be.
    pub(super) fn pet_act(&mut self, actor: ActorId, gen: u32) {
        let w = &self.world;
        let Some(a) = w.actor_ref(actor) else {
            return;
        };
        let (Some(life), ActorKind::Pet { owner, pet }) = (a.pet, a.kind) else {
            return;
        };
        if !a.alive || life.gen != gen || a.casting.is_some() || !w.in_combat() {
            return;
        }
        let s = Arc::clone(&w.s);
        let Some(def) = s.setup.data.pets.get(&pet) else {
            return;
        };
        let now = w.now;
        let mut next = None;
        for &spell in &def.autocast {
            let Some(sdef) = s.setup.data.spells.get(&spell) else {
                continue;
            };
            let Some(target) = self.world.pet_target(actor, owner, sdef) else {
                continue;
            };
            match self.world.pet_readiness(actor, spell, sdef, target) {
                Readiness::Now => {
                    self.start_pet_cast(owner, actor, spell, target);
                    return;
                }
                Readiness::In(d) => {
                    let at = now + d;
                    next = Some(next.map_or(at, |n: portunus_core::SimTime| n.min(at)));
                }
                Readiness::Blocked => {}
            }
        }
        if let Some(at) = next {
            self.world.schedule_pet_act(actor, at);
        }
    }

    fn start_pet_cast(
        &mut self,
        owner: Seat,
        actor: ActorId,
        spell: SpellId,
        target: Option<ActorId>,
    ) {
        let s = Arc::clone(&self.world.s);
        let Some(def) = s.setup.data.spells.get(&spell) else {
            return;
        };
        let now = self.world.now;
        if let Some(g) = &def.gcd {
            let end = now + self.world.spell_gcd(actor, spell, g);
            if let Some(life) = self.world.actor_mut(actor).and_then(|a| a.pet.as_mut()) {
                life.gcd_end = Some(end);
            }
        }
        let ev = CastEvent {
            seat: owner,
            actor,
            ability: None,
            spell,
            target,
            started: now,
            empower: None,
            spent: None,
            prerolled: false,
        };
        self.begin_cast(ev, CastOpts::default());
    }
}
