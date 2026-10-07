//! The kernel: [`Kernel`] implements [`Engine`] for any [`Mechanics`].
//!
//! The kernel reads the timing modifiers itself (`CastTimePct`, `GcdPct`,
//! `CooldownPct`, `ChargesAdd`, `CostPct`), because cast, GCD, and cooldown
//! timing are its job, and `ResourceMax` when it builds the seats; every
//! other modifier is left to the mechanics. Mechanics' calls
//! into [`EngineIo`] never call back into mechanics directly: the aura
//! changes and deaths they cause are queued and delivered once the current
//! callback returns.
//!
//! Not implemented yet, and rejected at setup or reported as
//! [`EngineError::Unsupported`] on first use: players' auto-attacks (pets'
//! work), channels, empowers, shared cooldown categories, cast lag, enemy
//! rules, `Fired` and `Delayed` engagement triggers, and aura value caps,
//! thresholds, and absorbs.

mod aura;
mod cast;
mod condition;
mod io;
mod pet;
mod queue;
mod setup;
mod swing;
mod world;

use std::sync::Arc;

use portunus_core::{ActorId, Seat, SimTime, SpellId, Trigger};
use portunus_scenario::resolved::{Segment, SpawnSet};

use crate::choice::{Choice, Wait};
use crate::error::{EngineError, IllegalChoice};
use crate::mask::{ActionMask, Readiness};
use crate::mechanics::{AuraApplication, CastEvent, EngineIo, Mechanics, RolledHit, TickEvent};
use crate::order::EventClass;
use crate::outcome::{Outcome, PullOutcome};
use crate::setup::RunSetup;
use crate::state::{ActorKind, AuraRef, LastCast, SeatPhase};
use crate::step::{DecisionRequest, Step, WakeReason};
use crate::trace::{CastEndReason, TraceEvent, TraceRecord};
use crate::Engine;
pub use world::World;

use cast::Need;
use io::Io;
use queue::{Event, Wake};
use world::{Actor, Batch, Casting, Followup, Seg};

/// How many queued reactions one callback may cause before the kernel
/// gives up with [`EngineError::Runaway`].
const CASCADE_LIMIT: usize = 100_000;

#[derive(Clone)]
pub struct Kernel<M> {
    mechanics: M,
    world: World,
}

impl<M: Mechanics> Kernel<M> {
    fn call(&mut self, f: impl FnOnce(&M, &mut dyn EngineIo)) {
        f(&self.mechanics, &mut Io { w: &mut self.world });
        self.drain();
    }

    fn drain(&mut self) {
        let mut handled = 0;
        while let Some(next) = self.world.followups.pop_front() {
            handled += 1;
            if handled > CASCADE_LIMIT {
                self.world.followups.clear();
                let now = self.world.now;
                self.world.fail(EngineError::Runaway(now));
                return;
            }
            let io = &mut Io { w: &mut self.world };
            match next {
                Followup::Changed(ev) => self.mechanics.aura_changed(io, &ev),
                Followup::Removed(ev) => self.mechanics.aura_removed(io, &ev),
                Followup::Died(ev) => self.mechanics.actor_died(io, &ev),
                Followup::PetExpired(ev) => self.mechanics.pet_expired(io, &ev),
            }
        }
    }

    // ---- legality ----

    fn readiness(&self, seat: Seat, ability: SpellId) -> (Readiness, WakeReason) {
        let (base, reason) = self.world.base_readiness(seat, ability);
        let gate = self.mechanics.gate(&self.world, seat, ability);
        let mut need = Need::new(self.world.now);
        need.add(base, reason);
        need.add(gate, WakeReason::Mechanics);
        need.finish(ability)
    }

    fn mask(&self, seat: Seat) -> ActionMask {
        let w = &self.world;
        let Some(abilities) = w.s.abilities.get(usize::from(seat.0)) else {
            return ActionMask {
                abilities: Default::default(),
                can_stop: false,
                can_wait_tick: false,
                targets: Vec::new(),
                cancel: Vec::new(),
            };
        };
        let me = w.seat_actor(seat);
        let a = w.actor_ref(me);
        let mut cancel: Vec<_> = a
            .map(|a| {
                a.auras
                    .iter()
                    .filter(|i| {
                        w.s.setup
                            .data
                            .auras
                            .get(&i.aura)
                            .is_some_and(|d| d.cancelable)
                    })
                    .map(|i| i.aura)
                    .collect()
            })
            .unwrap_or_default();
        cancel.dedup();
        ActionMask {
            abilities: abilities
                .iter()
                .map(|&s| (s, self.readiness(seat, s).0))
                .collect(),
            can_stop: a.is_some_and(|a| a.casting.is_some()),
            can_wait_tick: false,
            targets: if w.in_combat() {
                w.live_targets()
            } else {
                Vec::new()
            },
            cancel,
        }
    }

    fn actionable(&self, seat: Seat) -> bool {
        let w = &self.world;
        if w.actor_ref(w.seat_actor(seat))
            .is_some_and(|a| a.casting.is_some())
        {
            return true;
        }
        w.s.abilities[usize::from(seat.0)]
            .iter()
            .any(|&s| self.readiness(seat, s).0 == Readiness::Now)
    }

    fn check(&self, seat: Seat, choice: &Choice) -> Result<(), IllegalChoice> {
        let w = &self.world;
        let me = w.seat_actor(seat);
        match choice {
            Choice::Cast {
                ability, target, ..
            } => {
                if !w.s.abilities[usize::from(seat.0)].contains(ability) {
                    return Err(IllegalChoice::NotAnAbility(*ability));
                }
                let spell = w.resolved(seat, *ability);
                let Some(def) = w.s.setup.data.spells.get(&spell) else {
                    return Err(IllegalChoice::NotAnAbility(*ability));
                };
                if def.hostile && !w.in_combat() {
                    return Err(IllegalChoice::HostileBeforePull(*ability));
                }
                let (readiness, _) = self.readiness(seat, *ability);
                if readiness != Readiness::Now {
                    return Err(IllegalChoice::NotReady {
                        ability: *ability,
                        readiness,
                    });
                }
                w.resolve_target(seat, def, *target).map(|_| ())
            }
            Choice::Wait(Wait::ChannelTick) => Err(IllegalChoice::NoChannelTick),
            Choice::Wait(_) => Ok(()),
            Choice::StopCast => match w.actor_ref(me).and_then(|a| a.casting) {
                Some(_) => Ok(()),
                None => Err(IllegalChoice::NothingToStop),
            },
            Choice::SetTarget(id) => {
                if w.in_combat() && w.is_live_target(*id) {
                    Ok(())
                } else {
                    Err(IllegalChoice::NotATarget(*id))
                }
            }
            Choice::CancelAura(aura) => {
                if self.mask(seat).cancel.contains(aura) {
                    Ok(())
                } else {
                    Err(IllegalChoice::NotCancelable(*aura))
                }
            }
        }
    }

    // ---- the run ----

    fn start_segment(&mut self, i: usize) {
        let s = Arc::clone(&self.world.s);
        let segments = &s.setup.run.segments;
        let Some(seg) = segments.get(i) else {
            let completed = self.world.pulls.iter().all(|p| p.cleared.is_some());
            self.finish(completed);
            return;
        };
        let combat = segments[..i]
            .iter()
            .filter(|s| matches!(s, Segment::Combat(_)))
            .count() as u16;
        let now = self.world.now;
        match seg {
            Segment::Travel { duration, prepull } => {
                let combat_starts = now + *duration;
                let prepull_from = combat_starts - (*prepull).min(*duration);
                self.world.seg = Seg::Travel {
                    segment: i,
                    combat,
                    prepull_from,
                    combat_starts,
                };
                let next_is_combat = matches!(segments.get(i + 1), Some(Segment::Combat(_)));
                if next_is_combat {
                    self.world.queue.push(prepull_from, Event::PrepullOpen);
                }
                self.world.queue.push(combat_starts, Event::TravelEnd);
            }
            Segment::Combat(c) => self.begin_combat(i, combat, c),
        }
    }

    fn begin_combat(
        &mut self,
        segment: usize,
        combat: u16,
        c: &portunus_scenario::resolved::ResolvedCombat,
    ) {
        let s = Arc::clone(&self.world.s);
        let now = self.world.now;
        let first = self.world.actors.len();
        for (index, spawn) in c.spawns.iter().enumerate() {
            let phase = s
                .setup
                .enemies
                .enemies
                .get(&spawn.enemy)
                .and_then(|e| e.initial_phase.clone());
            let mut enemy = Actor::new(
                ActorKind::Enemy {
                    combat,
                    spawn: portunus_scenario::resolved::SpawnIndex(index as u16),
                },
                Arc::from(format!("{}/{}", c.pull.0, spawn.label.0).as_str()),
                spawn.max_health,
                1.0,
            );
            enemy.phase = phase;
            enemy.distance = spawn.distance;
            self.world.actors.push(enemy);
        }
        self.world.enemies = (first..self.world.actors.len())
            .map(|i| ActorId(i as u16))
            .collect();
        let deadline = now + c.timeout;
        self.world.seg = Seg::Combat {
            segment,
            combat,
            started: now,
            deadline,
        };
        self.world
            .queue
            .push(deadline, Event::CombatTimeout { combat });
        let mut elapsed = Vec::new();
        for spawn in &c.spawns {
            elapsed_marks(&spawn.engage, &mut elapsed);
        }
        for d in elapsed {
            self.world.queue.push(now + d, Event::EvalTriggers);
        }
        self.world.pulls.push(PullOutcome {
            pull: c.pull.clone(),
            started: now,
            cleared: None,
            deaths: 0,
        });
        self.world.record(TraceEvent::CombatStart { combat });
        self.call(|m, io| m.combat_started(io, combat));
        for i in 0..self.world.seats.len() {
            let seat = Seat(i as u8);
            let st = self.world.seat_mut(seat);
            if st.phase == SeatPhase::Idle {
                st.phase = SeatPhase::Locked;
            }
            self.world
                .notify(seat, WakeReason::CombatStart, true, None, now);
        }
        self.eval_triggers();
    }

    fn prepull_open(&mut self) {
        let now = self.world.now;
        for i in 0..self.world.seats.len() {
            let seat = Seat(i as u8);
            let st = self.world.seat_mut(seat);
            if st.phase == SeatPhase::Idle {
                st.phase = SeatPhase::Locked;
            }
            self.world
                .notify(seat, WakeReason::PrePull, true, None, now);
        }
    }

    fn eval_triggers(&mut self) {
        self.world.triggers_queued = None;
        let Seg::Combat { started, .. } = self.world.seg else {
            return;
        };
        let everyone_dead = self
            .world
            .seat_actors
            .iter()
            .all(|&a| self.world.actor_ref(a).is_some_and(|a| !a.alive));
        if everyone_dead {
            self.end_combat(false);
            return;
        }
        let s = Arc::clone(&self.world.s);
        let combat = self.world.combat_index().unwrap_or(0);
        let Some(Segment::Combat(c)) = s
            .setup
            .run
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Combat(_)))
            .nth(usize::from(combat))
        else {
            return;
        };
        loop {
            let ready: Vec<ActorId> = self
                .world
                .enemies
                .iter()
                .zip(&c.spawns)
                .filter(|(&e, spawn)| {
                    self.world
                        .actor_ref(e)
                        .is_some_and(|a| a.alive && !a.engaged)
                        && self.world.trigger_holds(&spawn.engage, started)
                })
                .map(|(&e, _)| e)
                .collect();
            if ready.is_empty() {
                break;
            }
            for e in ready {
                self.world.engage(e);
            }
            self.drain();
        }
        let cleared = self
            .world
            .enemies
            .iter()
            .all(|&e| self.world.actor_ref(e).is_some_and(|a| !a.alive));
        if cleared {
            self.end_combat(true);
        }
    }

    fn end_combat(&mut self, cleared: bool) {
        let Seg::Combat {
            segment, combat, ..
        } = self.world.seg
        else {
            return;
        };
        let now = self.world.now;
        if let Some(p) = self.world.pulls.last_mut() {
            p.cleared = cleared.then_some(now);
        }
        self.world.record(TraceEvent::CombatEnd { combat, cleared });
        self.call(|m, io| m.combat_ended(io, combat, cleared));
        for i in 0..self.world.seats.len() {
            let seat = Seat(i as u8);
            let me = self.world.seat_actor(seat);
            self.world.cancel_cast(me, CastEndReason::Interrupted);
            let st = self.world.seat_mut(seat);
            if st.phase != SeatPhase::Dead {
                st.phase = SeatPhase::Idle;
            }
            st.wait = None;
            st.armed = None;
            st.gen += 1;
        }
        for pet in self.world.all_pets() {
            self.world.cancel_cast(pet, CastEndReason::Interrupted);
            self.world.stop_swings(pet);
        }
        self.world.projectiles.clear();
        self.world.flights.clear();
        self.world.settle_departures();
        self.drain();
        if cleared {
            self.world.seg = Seg::Finished;
            self.start_segment(segment + 1);
        } else {
            self.finish(false);
        }
    }

    fn finish(&mut self, completed: bool) {
        self.world.seg = Seg::Finished;
        self.world.finished = Some(Outcome {
            completed,
            end_time: self.world.now,
            pulls: self.world.pulls.clone(),
            seats: self.world.seat_outcomes(),
        });
    }

    // ---- casts ----

    fn start_cast(&mut self, seat: Seat, ability: SpellId, target: Option<ActorId>) {
        let s = Arc::clone(&self.world.s);
        let now = self.world.now;
        let actor = self.world.seat_actor(seat);
        let spell = self.world.resolved(seat, ability);
        let Some(def) = s.setup.data.spells.get(&spell) else {
            return;
        };
        if let Some(g) = &def.gcd {
            let gcd = self.world.spell_gcd(actor, spell, g);
            self.world.seat_mut(seat).gcd_end = Some(now + gcd);
        }
        self.begin_cast(CastEvent {
            seat,
            actor,
            ability: Some(ability),
            spell,
            target,
            started: now,
            empower: None,
            spent: None,
            prerolled: false,
        });
    }

    /// Start a seat's or pet's cast once its GCD is set: hard casts wait
    /// for their cast time, instants complete now.
    fn begin_cast(&mut self, ev: CastEvent) {
        let now = self.world.now;
        let (actor, spell, target) = (ev.actor, ev.spell, ev.target);
        let cast_time = self.world.cast_time_of(actor, spell, target);
        self.world.record(TraceEvent::CastStart {
            actor,
            spell,
            target,
        });
        self.call(|m, io| m.cast_started(io, &ev));
        if cast_time.millis() > 0 {
            let Some(a) = self.world.actor_mut(actor) else {
                return;
            };
            a.cast_seq += 1;
            let seq = a.cast_seq;
            let ends = now + cast_time;
            a.casting = Some(Casting { ev, ends, seq });
            self.world
                .queue
                .push(ends, Event::CastComplete { actor, cast: seq });
        } else {
            self.complete_cast(ev, false);
        }
    }

    fn complete_cast(&mut self, mut ev: CastEvent, hard: bool) {
        let s = Arc::clone(&self.world.s);
        let Some(def) = s.setup.data.spells.get(&ev.spell) else {
            return;
        };
        let now = self.world.now;
        ev.spent = self.world.pay_costs(ev.actor, ev.spell, def, ev.target);
        if let Some(cd) = &def.cooldown {
            self.world.consume_charge(ev.actor, ev.spell, cd);
        }
        let by_pet = self.world.is_pet(ev.actor);
        if !by_pet {
            let st = self.world.seat_mut(ev.seat);
            st.last_cast = Some(LastCast {
                spell: ev.spell,
                at: now,
            });
            st.outcome.casts += 1;
        }
        self.world.record(TraceEvent::CastEnd {
            actor: ev.actor,
            spell: ev.spell,
            reason: CastEndReason::Completed,
        });
        self.call(|m, io| m.cast_completed(io, &ev));
        self.world.launch(ev, def);
        if by_pet {
            self.world.schedule_pet_act(ev.actor, now);
        } else if hard {
            let st = self.world.seat_mut(ev.seat);
            if st.phase == SeatPhase::Committed {
                st.phase = SeatPhase::Locked;
            }
            self.world
                .notify(ev.seat, WakeReason::CastEnd, true, None, now);
        }
    }

    fn resolve_triggered(
        &mut self,
        caster: ActorId,
        spell: SpellId,
        target: Option<ActorId>,
        rolled: Option<Vec<RolledHit>>,
    ) {
        let s = Arc::clone(&self.world.s);
        let Some(def) = s.setup.data.spells.get(&spell) else {
            self.world
                .unsupported("triggering a spell missing from the game data");
            return;
        };
        let Some(seat) = self.world.owner_seat(caster) else {
            self.world.unsupported("spells triggered for enemies");
            return;
        };
        if !self.world.actor_ref(caster).is_some_and(|a| a.alive)
            && !self.world.is_departing(caster)
        {
            return;
        }
        let ev = CastEvent {
            seat,
            actor: caster,
            ability: None,
            spell,
            target,
            started: self.world.now,
            empower: None,
            spent: None,
            prerolled: rolled.is_some(),
        };
        self.world.record(TraceEvent::CastEnd {
            actor: caster,
            spell,
            reason: CastEndReason::Completed,
        });
        if let Some(hits) = rolled {
            self.world.stashed = hits;
        }
        self.call(|m, io| m.cast_completed(io, &ev));
        self.world.launch(ev, def);
    }

    // ---- events ----

    fn dispatch(&mut self, event: Event) {
        self.handle(event);
        self.world.settle_departures();
    }

    fn handle(&mut self, event: Event) {
        match event {
            Event::PrepullOpen => self.prepull_open(),
            Event::TravelEnd => {
                if let Seg::Travel { segment, .. } = self.world.seg {
                    self.start_segment(segment + 1);
                }
            }
            Event::CombatTimeout { combat } => {
                if self.world.combat_index() == Some(combat) {
                    self.end_combat(false);
                }
            }
            Event::CastComplete { actor, cast } => {
                let Some(a) = self.world.actor_mut(actor) else {
                    return;
                };
                if a.casting.is_some_and(|c| c.seq == cast) {
                    if let Some(c) = a.casting.take() {
                        self.complete_cast(c.ev, true);
                    }
                }
            }
            Event::ProjectileLand { id } => {
                let Some(i) = self.world.flights.iter().position(|(f, ..)| *f == id) else {
                    return;
                };
                let (_, ev, hits) = self.world.flights.remove(i);
                let flight = self.world.projectiles.remove(i);
                self.call(|m, io| m.projectile_landed(io, &ev, &flight, &hits));
                self.world.settle_departures();
                self.drain();
            }
            Event::Triggered {
                caster,
                spell,
                target,
                rolled,
            } => self.resolve_triggered(caster, spell, target, rolled),
            Event::AuraTick { aura, uid } => {
                let Some(index) = self.world.take_tick(aura, uid) else {
                    return;
                };
                let tick = TickEvent {
                    aura,
                    index,
                    fraction: 1.0,
                };
                self.call(|m, io| m.periodic_tick(io, &tick));
                self.world.continue_ticking(aura, uid);
            }
            Event::AuraExpire { aura, uid } => {
                if let Some((index, fraction)) = self.world.final_tick(aura, uid) {
                    let tick = TickEvent {
                        aura,
                        index,
                        fraction,
                    };
                    self.call(|m, io| m.periodic_tick(io, &tick));
                }
                self.world.expire(aura, uid);
                self.drain();
            }
            Event::AuraStackExpire { aura, uid } => {
                self.world.expire_stacks(aura, uid);
                self.drain();
            }
            Event::Timer { id } => {
                let Some(i) = self.world.timer_ids.iter().position(|&t| t == id) else {
                    return;
                };
                self.world.timer_ids.remove(i);
                let pending = self.world.timers.remove(i);
                self.call(|m, io| m.timer(io, &pending.timer));
            }
            Event::EvalTriggers => self.eval_triggers(),
            Event::CooldownReady { actor, spell, gen } => {
                self.world.cooldown_ready(actor, spell, gen);
            }
            Event::PetAct { actor, gen } => self.pet_act(actor, gen),
            Event::Swing { actor, hand, gen } => self.swing(actor, hand, gen),
            Event::PetExpire { actor } => {
                self.world.pet_expire(actor);
                self.drain();
            }
            Event::Recheck { .. } | Event::Deliver(_) => {
                let mut ignored = Vec::new();
                self.accept(event, &mut ignored);
            }
        }
    }

    // ---- decisions ----

    fn recheck(&mut self, seat: Seat, gen: u32) {
        let now = self.world.now;
        let st = &self.world.seats[usize::from(seat.0)];
        if st.gen != gen {
            return;
        }
        let reason = match (st.phase, &st.wait) {
            (SeatPhase::Locked, _) | (SeatPhase::Waiting, Some(Wait::NextEvent)) => {
                match st.armed {
                    Some((t, reason)) if t <= now => reason,
                    _ => return,
                }
            }
            (SeatPhase::Waiting, Some(Wait::Condition(c))) => {
                if !self.world.condition_now(seat, c) {
                    return;
                }
                WakeReason::ConditionMet
            }
            _ => return,
        };
        self.world.seat_mut(seat).phase = SeatPhase::Deciding;
        self.world.notify(seat, reason, true, Some(gen), now);
    }

    fn accept(&mut self, event: Event, out: &mut Vec<Wake>) {
        match event {
            Event::Recheck { seat, gen } => self.recheck(seat, gen),
            Event::Deliver(w) => {
                self.world.perceive(w.seat, w.perception);
                let st = &self.world.seats[usize::from(w.seat.0)];
                if matches!(st.phase, SeatPhase::Idle | SeatPhase::Dead)
                    || w.gate.is_some_and(|g| g != st.gen)
                {
                    return;
                }
                if !self.actionable(w.seat) {
                    let own = w.gate.is_some() || st.phase == SeatPhase::Deciding;
                    let st = self.world.seat_mut(w.seat);
                    if own && st.phase != SeatPhase::Committed {
                        st.phase = SeatPhase::Locked;
                        st.wait = None;
                        st.armed = None;
                    }
                    return;
                }
                out.push(w);
            }
            other => self.dispatch(other),
        }
    }

    fn collect(&mut self, first: Event) {
        let now = self.world.now;
        let mut wakes = Vec::new();
        self.accept(first, &mut wakes);
        while let Some(key) = self.world.queue.peek_key() {
            if key.time != now || key.class != EventClass::Decisions {
                break;
            }
            let Some((_, ev)) = self.world.queue.pop() else {
                break;
            };
            self.accept(ev, &mut wakes);
        }
        if wakes.is_empty() {
            return;
        }
        wakes.sort_by_key(|w| w.seat);
        wakes.dedup_by_key(|w| w.seat);
        for w in &wakes {
            let casting = self
                .world
                .actor_ref(self.world.seat_actor(w.seat))
                .is_some_and(|a| a.casting.is_some());
            let st = self.world.seat_mut(w.seat);
            st.phase = if casting {
                SeatPhase::Committed
            } else {
                SeatPhase::Deciding
            };
            st.gen += 1;
            st.armed = None;
            st.wait = None;
        }
        self.world.batch = Some(Batch {
            answers: vec![None; wakes.len()],
            requests: wakes
                .iter()
                .map(|w| DecisionRequest {
                    seat: w.seat,
                    reason: w.reason,
                    anticipated: w.anticipated,
                    event_at: w.event_at,
                    now,
                })
                .collect(),
        });
    }

    fn apply(&mut self, req: DecisionRequest, choice: Choice) {
        let seat = req.seat;
        if self.check(seat, &choice).is_err() {
            self.world.deliver_now(seat, req.reason);
            return;
        }
        let now = self.world.now;
        let me = self.world.seat_actor(seat);
        self.world.record(TraceEvent::Decision {
            seat,
            choice: choice.clone(),
        });
        match choice {
            Choice::Cast {
                ability, target, ..
            } => {
                let s = Arc::clone(&self.world.s);
                let spell = self.world.resolved(seat, ability);
                let Some(def) = s.setup.data.spells.get(&spell) else {
                    return;
                };
                let Ok(target) = self.world.resolve_target(seat, def, target) else {
                    return;
                };
                self.start_cast(seat, ability, target);
                let casting = self
                    .world
                    .actor_ref(me)
                    .is_some_and(|a| a.casting.is_some());
                let off_gcd = s.abilities[usize::from(seat.0)]
                    .iter()
                    .any(|&a| self.readiness(seat, a).0 == Readiness::Now);
                let st = self.world.seat_mut(seat);
                st.phase = if casting {
                    SeatPhase::Committed
                } else if off_gcd {
                    SeatPhase::Deciding
                } else {
                    SeatPhase::Locked
                };
                if off_gcd {
                    self.world.deliver_now(seat, WakeReason::OffGcdReady);
                }
            }
            Choice::Wait(wait) => {
                let casting = self
                    .world
                    .actor_ref(me)
                    .is_some_and(|a| a.casting.is_some());
                let gcd_end = self.world.seats[usize::from(seat.0)].gcd_end;
                let st = self.world.seat_mut(seat);
                st.waited = Some((wait.clone(), now));
                st.wait = Some(wait.clone());
                st.phase = if casting {
                    SeatPhase::Committed
                } else {
                    SeatPhase::Waiting
                };
                let gen = st.gen;
                match wait {
                    Wait::Until(t) => {
                        self.world.notify(
                            seat,
                            WakeReason::WaitElapsed,
                            true,
                            Some(gen),
                            t.max(now),
                        );
                    }
                    Wait::GcdEnd => {
                        let at = gcd_end.map_or(now, |g| g.max(now));
                        self.world
                            .notify(seat, WakeReason::GcdEnd, true, Some(gen), at);
                    }
                    Wait::NextEvent | Wait::Condition(_) | Wait::ChannelTick => {}
                }
            }
            Choice::StopCast => {
                self.world.cancel_cast(me, CastEndReason::Stopped);
                self.world.seat_mut(seat).phase = SeatPhase::Deciding;
                self.world.deliver_now(seat, WakeReason::CastStopped);
            }
            Choice::SetTarget(id) => {
                if let Some(a) = self.world.actor_mut(me) {
                    a.target = Some(id);
                }
                for pet in self.world.pets[usize::from(seat.0)].clone() {
                    self.world.wake_pet(pet);
                }
                self.world.deliver_now(seat, WakeReason::FreeAction);
            }
            Choice::CancelAura(aura) => {
                self.world.remove_aura(me, aura, None);
                self.drain();
                self.world.deliver_now(seat, WakeReason::FreeAction);
            }
        }
    }

    fn apply_batch(&mut self) {
        let Some(batch) = self.world.batch.take() else {
            return;
        };
        for (req, choice) in batch.requests.into_iter().zip(batch.answers) {
            if let Some(choice) = choice {
                self.apply(req, choice);
            }
        }
    }

    /// The next wake an armed seat should expect, if nothing else happens.
    fn arm(&self, seat: Seat, include_now: bool) -> Option<(SimTime, WakeReason)> {
        let now = self.world.now;
        self.world.s.abilities[usize::from(seat.0)]
            .iter()
            .filter_map(|&s| match self.readiness(seat, s) {
                (Readiness::Now, reason) => include_now.then_some((now, reason)),
                (Readiness::In(d), reason) => Some((now + d, reason)),
                (Readiness::Blocked, _) => None,
            })
            .min_by_key(|&(t, _)| t)
    }

    /// Re-predict every armed seat's next wake after the world changed.
    /// For readiness wakes, one already due stands: its recheck is still
    /// queued at this timestamp and will deliver, and re-predicting now
    /// would skip past whatever just became ready.
    fn rearm(&mut self) {
        let now = self.world.now;
        for i in 0..self.world.seats.len() {
            let seat = Seat(i as u8);
            let st = &self.world.seats[i];
            let readiness_wake = matches!(
                (st.phase, &st.wait),
                (SeatPhase::Locked, _) | (SeatPhase::Waiting, Some(Wait::NextEvent))
            );
            if readiness_wake && st.armed.is_some_and(|(t, _)| t <= now) {
                continue;
            }
            let target = match (st.phase, &st.wait) {
                (SeatPhase::Locked, _) => self.arm(seat, true),
                (SeatPhase::Waiting, Some(Wait::NextEvent)) => self.arm(seat, false),
                (SeatPhase::Waiting, Some(Wait::Condition(c))) => self
                    .world
                    .solve(seat, c)
                    .map(|t| (t, WakeReason::ConditionMet)),
                _ => continue,
            };
            if target == self.world.seats[i].armed {
                continue;
            }
            let st = self.world.seat_mut(seat);
            st.gen += 1;
            st.armed = target;
            let gen = st.gen;
            if let Some((t, _)) = target {
                self.world.queue.push(t, Event::Recheck { seat, gen });
            }
        }
    }
}

/// Every `Elapsed` duration in a trigger, so the kernel can re-evaluate at
/// those moments.
fn elapsed_marks(t: &Trigger<SpawnSet>, out: &mut Vec<portunus_core::SimDuration>) {
    match t {
        Trigger::Elapsed(d) => out.push(*d),
        Trigger::All(ts) | Trigger::Any(ts) => {
            for t in ts {
                elapsed_marks(t, out);
            }
        }
        _ => {}
    }
}

impl World {
    fn members(&self, set: &SpawnSet) -> Vec<ActorId> {
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

    fn trigger_holds(&self, t: &Trigger<SpawnSet>, started: SimTime) -> bool {
        let actors = |set: &SpawnSet| -> Vec<&Actor> {
            self.members(set)
                .into_iter()
                .filter_map(|e| self.actor_ref(e))
                .collect()
        };
        match t {
            Trigger::Now => true,
            Trigger::Never => false,
            Trigger::Elapsed(d) => self.now >= started + *d,
            Trigger::HpFracBelow { who, frac } => {
                let group = actors(who);
                let max: u64 = group.iter().map(|a| a.max_health).sum();
                let hp: u64 = group.iter().map(|a| a.health).sum();
                max > 0 && (hp as f64) < *frac * max as f64
            }
            Trigger::AliveAtMost { who, count } => {
                actors(who).iter().filter(|a| a.alive).count() <= *count as usize
            }
            Trigger::Died(who) => actors(who).iter().all(|a| !a.alive),
            Trigger::All(ts) => ts.iter().all(|t| self.trigger_holds(t, started)),
            Trigger::Any(ts) => ts.iter().any(|t| self.trigger_holds(t, started)),
            Trigger::Fired { .. } | Trigger::Delayed { .. } => false,
        }
    }
}

impl<M: Mechanics> Engine for Kernel<M> {
    type Mechanics = M;
    type State = World;

    fn new(setup: RunSetup, mechanics: M) -> Result<Self, EngineError> {
        setup::validate(&setup)?;
        let mut kernel = Kernel {
            mechanics,
            world: setup::build(setup),
        };
        let s = Arc::clone(&kernel.world.s);
        for (i, seat) in s.setup.seats.iter().enumerate() {
            let me = ActorId(i as u16);
            let auras = seat
                .template
                .passive_auras
                .iter()
                .chain(&s.setup.externals.party_auras);
            for &aura in auras {
                kernel.world.apply_aura(AuraApplication {
                    aura: AuraRef {
                        holder: me,
                        aura,
                        source: me,
                    },
                    stacks: 1,
                    duration: None,
                });
            }
        }
        kernel.drain();
        for (i, seat) in s.setup.seats.iter().enumerate() {
            if let Some(pet) = seat.template.permanent_pet {
                kernel.world.summon(Seat(i as u8), pet, 1, None);
            }
        }
        kernel.drain();
        kernel.start_segment(0);
        kernel.rearm();
        match kernel.world.fault.clone() {
            Some(e) => Err(e),
            None => Ok(kernel),
        }
    }

    fn advance(&mut self) -> Result<Step, EngineError> {
        loop {
            if let Some(e) = &self.world.fault {
                return Err(e.clone());
            }
            if let Some(o) = &self.world.finished {
                return Ok(Step::Done(o.clone()));
            }
            if let Some(b) = &self.world.batch {
                let next = b.answers.iter().position(Option::is_none).unwrap_or(0);
                return Ok(Step::Decide(b.requests[next]));
            }
            let Some((key, event)) = self.world.queue.pop() else {
                let completed = self.world.pulls.iter().all(|p| p.cleared.is_some());
                self.finish(completed);
                continue;
            };
            self.world.now = key.time;
            if key.class == EventClass::Decisions {
                self.collect(event);
            } else {
                self.dispatch(event);
            }
            self.rearm();
        }
    }

    fn submit(&mut self, seat: Seat, choice: Choice) -> Result<(), EngineError> {
        let Some(batch) = &self.world.batch else {
            return Err(EngineError::NotAwaiting);
        };
        let Some(next) = batch.answers.iter().position(Option::is_none) else {
            return Err(EngineError::NotAwaiting);
        };
        let req = batch.requests[next];
        if req.seat != seat {
            return Err(EngineError::WrongSeat(seat));
        }
        self.check(seat, &choice)
            .map_err(|reason| EngineError::Illegal { seat, reason })?;
        if let Choice::Wait(w) = &choice {
            let now = self.world.now;
            let own_wake = matches!(
                (w, req.reason),
                (Wait::Until(_), WakeReason::WaitElapsed)
                    | (Wait::GcdEnd, WakeReason::GcdEnd)
                    | (Wait::Condition(_), WakeReason::ConditionMet)
            );
            let repeat = self.world.seats[usize::from(seat.0)]
                .waited
                .as_ref()
                .is_some_and(|(prev, at)| prev == w && *at == now);
            if own_wake && repeat {
                return Err(EngineError::Livelock(seat));
            }
        }
        let Some(batch) = &mut self.world.batch else {
            return Err(EngineError::NotAwaiting);
        };
        batch.answers[next] = Some(choice);
        if batch.answers.iter().all(Option::is_some) {
            self.apply_batch();
            self.rearm();
        }
        Ok(())
    }

    fn legal(&self, seat: Seat) -> ActionMask {
        self.mask(seat)
    }

    fn state(&self) -> &World {
        &self.world
    }

    fn trace_hash(&self) -> u64 {
        self.world.trace.hash
    }

    fn drain_trace(&mut self) -> Vec<TraceRecord> {
        self.world
            .trace
            .records
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
}
