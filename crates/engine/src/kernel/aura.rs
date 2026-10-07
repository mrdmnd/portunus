//! Aura bookkeeping: application, refresh, stacks, ticks, expiry, values,
//! and proc records.

use std::sync::Arc;

use portunus_core::{ActorId, AuraId, SimDuration};
use portunus_gamedata::aura::{Periodic, RefreshRule};
use portunus_gamedata::effect::ProcChance;

use crate::mechanics::{AuraApplication, AuraChange, AuraEvent, AuraRemoval};
use crate::state::{AuraInstance, AuraRef, DeckView, ListenerRef, ProcView};

use crate::step::WakeReason;
use crate::trace::TraceEvent;

use super::queue::Event;
use super::world::{millis_round, AuraMeta, Followup, Tick, World};

fn full_deck(chance: ProcChance) -> Option<DeckView> {
    match chance {
        ProcChance::Deck { successes, size } => Some(DeckView {
            cards: size,
            successes: successes.min(size),
        }),
        _ => None,
    }
}

/// Derive an independently stacked instance's stacks and expiries from its
/// per-stack timers. A no-op when it has none.
fn sync_stacks(inst: &mut AuraInstance, meta: &AuraMeta) {
    let (Some(&oldest), Some(&newest)) = (meta.stack_expiries.first(), meta.stack_expiries.last())
    else {
        return;
    };
    inst.stacks = u8::try_from(meta.stack_expiries.len()).unwrap_or(u8::MAX);
    inst.expires = Some(newest);
    inst.next_stack_expires = (oldest < newest).then_some(oldest);
}

fn period(p: &Periodic, haste: f64) -> SimDuration {
    let ms = f64::from(p.period.millis());
    let ms = if p.hasted { ms / haste } else { ms };
    millis_round(ms).max(SimDuration(1))
}

impl World {
    pub(crate) fn find_aura(&self, r: AuraRef) -> Option<usize> {
        self.actor_ref(r.holder)?
            .auras
            .iter()
            .position(|a| a.aura == r.aura && a.source == r.source)
    }

    pub(crate) fn source_haste(&self, r: AuraRef) -> f64 {
        self.actor_ref(r.source).map_or(1.0, |a| a.haste)
    }

    pub(crate) fn apply_aura(&mut self, app: AuraApplication) {
        if self.affects_movement(app.aura.aura) {
            self.before_movement_aura(app.aura.holder);
            self.apply_aura_now(app);
            self.after_movement_aura(app.aura.holder);
        } else {
            self.apply_aura_now(app);
        }
    }

    fn apply_aura_now(&mut self, app: AuraApplication) {
        let s = Arc::clone(&self.s);
        let r = app.aura;
        let Some(def) = s.setup.data.auras.get(&r.aura) else {
            self.unsupported("applying an aura missing from the game data");
            return;
        };
        let now = self.now;
        let Some(holder) = self.actor_ref(r.holder) else {
            return;
        };
        if !holder.alive {
            return;
        }
        if def
            .blocked_by
            .is_some_and(|b| holder.auras.iter().any(|a| a.aura == b))
        {
            return;
        }
        let duration = app.duration.or(def.duration);
        let max_stacks = def.max_stacks.max(1);
        let timed_stacks = |n: u8| match duration {
            Some(d) if def.refresh == RefreshRule::Ironfur => vec![now + d; usize::from(n)],
            _ => Vec::new(),
        };
        let pmultiplier = self.pmultiplier_of(r.source, r.aura);
        let haste = self.source_haste(r);
        match self.find_aura(r) {
            None => {
                let uid = self.fresh_id();
                let stacks = app.stacks.clamp(1, max_stacks);
                let tick = def.periodic.as_ref().map(|p| Tick {
                    last_at: now,
                    period: period(p, haste),
                    next_at: None,
                    index: 0,
                });
                let decks: Vec<Option<DeckView>> = (0..def.listeners.len())
                    .map(|index| self.deck_for(r.holder, r.aura, index as u8))
                    .collect();
                let Some(holder) = self.actor_mut(r.holder) else {
                    return;
                };
                holder.auras.push(AuraInstance {
                    aura: r.aura,
                    source: r.source,
                    stacks,
                    expires: duration.map(|d| now + d),
                    next_stack_expires: None,
                    value: 0.0,
                    pmultiplier,
                });
                holder.meta.push(AuraMeta {
                    uid,
                    tick,
                    stack_expiries: timed_stacks(stacks),
                });
                for (index, deck) in decks.into_iter().enumerate() {
                    holder.procs.push(ProcView {
                        listener: ListenerRef {
                            aura: r,
                            index: index as u8,
                        },
                        last_attempt: None,
                        last_proc: None,
                        icd_ready_at: None,
                        deck,
                    });
                }
                self.schedule_aura(r);
                self.changed(r, 0, stacks);
                if let Some(seat) = self.player_seat(r.holder) {
                    self.notify(seat, WakeReason::AuraGained(r.aura), false, None, now);
                }
            }
            Some(i) => {
                let Some(holder) = self.actor_mut(r.holder) else {
                    return;
                };
                let previous = holder.auras[i].stacks;
                let added = timed_stacks(app.stacks);
                if !added.is_empty() {
                    // At the cap, each new stack replaces the oldest.
                    let expiries = &mut holder.meta[i].stack_expiries;
                    expiries.extend(added);
                    let excess = expiries.len().saturating_sub(usize::from(max_stacks));
                    expiries.drain(..excess);
                    holder.auras[i].pmultiplier = pmultiplier;
                    sync_stacks(&mut holder.auras[i], &holder.meta[i]);
                    let stacks = holder.auras[i].stacks;
                    self.schedule_aura(r);
                    self.changed(r, previous, stacks);
                    return;
                }
                let inst = &mut holder.auras[i];
                inst.stacks = previous.saturating_add(app.stacks).min(max_stacks);
                inst.expires = duration.map(|d| {
                    let remaining = inst
                        .expires
                        .map_or(SimDuration::ZERO, |e| e.saturating_since(now));
                    now + match def.refresh {
                        RefreshRule::Pandemic(f) => {
                            let cap = millis_round(f64::from(d.millis()) * f);
                            d + remaining.min(cap)
                        }
                        RefreshRule::Extend => d + remaining,
                        RefreshRule::Replace | RefreshRule::Ironfur => d,
                    }
                });
                inst.pmultiplier = pmultiplier;
                let stacks = inst.stacks;
                self.schedule_aura(r);
                self.changed(r, previous, stacks);
            }
        }
    }

    fn changed(&mut self, r: AuraRef, previous_stacks: u8, stacks: u8) {
        self.followups.push_back(Followup::Changed(AuraChange {
            aura: r,
            previous_stacks,
            stacks,
        }));
        self.record(TraceEvent::AuraApplied {
            holder: r.holder,
            aura: r.aura,
            stacks,
        });
    }

    /// Queue the instance's expiry and, if none is pending, its next tick.
    pub(crate) fn schedule_aura(&mut self, r: AuraRef) {
        let Some(i) = self.find_aura(r) else { return };
        let Some(a) = self.actor_mut(r.holder) else {
            return;
        };
        let expires = a.auras[i].expires;
        let stack_at = a.auras[i].next_stack_expires;
        let meta = &mut a.meta[i];
        let uid = meta.uid;
        let mut tick_at = None;
        if let Some(tick) = &mut meta.tick {
            if tick.next_at.is_none() {
                let next = tick.last_at + tick.period;
                if expires.is_none_or(|e| next < e) {
                    tick.next_at = Some(next);
                    tick_at = Some(next);
                }
            }
        }
        if let Some(e) = expires {
            self.queue.push(e, Event::AuraExpire { aura: r, uid });
        }
        if let Some(t) = stack_at {
            self.queue.push(t, Event::AuraStackExpire { aura: r, uid });
        }
        if let Some(t) = tick_at {
            self.queue.push(t, Event::AuraTick { aura: r, uid });
        }
    }

    pub(crate) fn remove_instance(&mut self, holder: ActorId, i: usize, reason: AuraRemoval) {
        let Some(a) = self.actor_mut(holder) else {
            return;
        };
        if i >= a.auras.len() {
            return;
        }
        let aura = a.auras[i].aura;
        let movement = self.affects_movement(aura);
        if movement {
            self.before_movement_aura(holder);
        }
        let Some(a) = self.actor_mut(holder) else {
            return;
        };
        let inst = a.auras.remove(i);
        a.meta.remove(i);
        let r = AuraRef {
            holder,
            aura: inst.aura,
            source: inst.source,
        };
        a.procs.retain(|p| p.listener.aura != r);
        self.followups
            .push_back(Followup::Removed(AuraEvent { aura: r, reason }));
        self.record(TraceEvent::AuraRemoved {
            holder,
            aura: inst.aura,
        });
        if movement {
            self.after_movement_aura(holder);
        }
    }

    pub(crate) fn remove_aura(&mut self, holder: ActorId, aura: AuraId, source: Option<ActorId>) {
        while let Some(i) = self.actor_ref(holder).and_then(|a| {
            a.auras
                .iter()
                .rposition(|x| x.aura == aura && source.is_none_or(|s| x.source == s))
        }) {
            self.remove_instance(holder, i, AuraRemoval::Removed);
        }
    }

    pub(crate) fn remove_stacks(&mut self, r: AuraRef, stacks: u8) {
        let Some(i) = self.find_aura(r) else { return };
        let Some(a) = self.actor_mut(r.holder) else {
            return;
        };
        let inst = &mut a.auras[i];
        if inst.stacks <= stacks {
            self.remove_instance(r.holder, i, AuraRemoval::Removed);
        } else {
            let previous = inst.stacks;
            inst.stacks -= stacks;
            let expiries = &mut a.meta[i].stack_expiries;
            if !expiries.is_empty() {
                expiries.drain(..usize::from(stacks).min(expiries.len()));
                sync_stacks(&mut a.auras[i], &a.meta[i]);
            }
            let now = a.auras[i].stacks;
            self.changed(r, previous, now);
        }
    }

    /// Drop the stacks of an independently stacked aura whose timers are
    /// up. Its last stack goes with the aura's own expiry.
    pub(crate) fn expire_stacks(&mut self, r: AuraRef, uid: u32) {
        let now = self.now;
        let Some(i) = self.find_aura(r) else { return };
        let Some(a) = self.actor_mut(r.holder) else {
            return;
        };
        let meta = &mut a.meta[i];
        if meta.uid != uid {
            return;
        }
        let due = meta
            .stack_expiries
            .iter()
            .take_while(|&&t| t <= now)
            .count();
        if due == 0 || due == meta.stack_expiries.len() {
            return;
        }
        meta.stack_expiries.drain(..due);
        let previous = a.auras[i].stacks;
        sync_stacks(&mut a.auras[i], &a.meta[i]);
        let stacks = a.auras[i].stacks;
        if let Some(t) = a.auras[i].next_stack_expires {
            self.queue.push(t, Event::AuraStackExpire { aura: r, uid });
        }
        self.changed(r, previous, stacks);
    }

    pub(crate) fn extend_aura(&mut self, r: AuraRef, by: SimDuration) {
        let Some(i) = self.find_aura(r) else { return };
        let Some(a) = self.actor_mut(r.holder) else {
            return;
        };
        if let Some(e) = &mut a.auras[i].expires {
            *e += by;
            for t in &mut a.meta[i].stack_expiries {
                *t += by;
            }
            sync_stacks(&mut a.auras[i], &a.meta[i]);
            self.schedule_aura(r);
        }
    }

    pub(crate) fn add_aura_value(&mut self, r: AuraRef, delta: f64) {
        let s = Arc::clone(&self.s);
        let Some(def) = s.setup.data.auras.get(&r.aura) else {
            return;
        };
        if def.value.as_ref().is_some_and(|v| {
            v.cap.is_some()
                || v.threshold.is_some()
                || matches!(
                    v.kind,
                    portunus_gamedata::aura::AuraValueKind::Absorb { .. }
                )
        }) {
            self.unsupported("aura value caps, thresholds, and absorbs");
            return;
        }
        if self.find_aura(r).is_none() {
            self.apply_aura(AuraApplication {
                aura: r,
                stacks: 1,
                duration: None,
            });
        }
        let Some(i) = self.find_aura(r) else { return };
        if let Some(a) = self.actor_mut(r.holder) {
            a.auras[i].value += delta;
        }
    }

    pub(crate) fn record_proc(&mut self, listener: ListenerRef, procced: bool) {
        let s = Arc::clone(&self.s);
        let Some(def) = s
            .setup
            .data
            .auras
            .get(&listener.aura.aura)
            .and_then(|d| d.listeners.get(usize::from(listener.index)))
        else {
            return;
        };
        let now = self.now;
        let Some(a) = self.actor_mut(listener.aura.holder) else {
            return;
        };
        let Some(p) = a.procs.iter_mut().find(|p| p.listener == listener) else {
            return;
        };
        p.last_attempt = Some(now);
        if procced {
            p.last_proc = Some(now);
            p.icd_ready_at = def.internal_cooldown.map(|d| now + d);
        }
        let Some(deck) = &mut p.deck else { return };
        deck.cards = deck.cards.saturating_sub(1);
        if procced {
            deck.successes = deck.successes.saturating_sub(1);
        }
        if deck.cards == 0 {
            p.deck = full_deck(def.chance);
        }
        if let Some(deck) = p.deck {
            let key = (listener.aura.holder, listener.aura.aura, listener.index);
            self.decks.insert(key, deck);
        }
    }

    /// A holder's deck for one listener: where it left off, or a fresh one.
    /// None for a listener drawing from another's.
    fn deck_for(&self, holder: ActorId, aura: AuraId, index: u8) -> Option<DeckView> {
        let listener = self
            .s
            .setup
            .data
            .auras
            .get(&aura)?
            .listeners
            .get(usize::from(index))?;
        if listener.shared_with.is_some() {
            return None;
        }
        let fresh = full_deck(listener.chance)?;
        Some(
            self.decks
                .get(&(holder, aura, index))
                .copied()
                .unwrap_or(fresh),
        )
    }

    /// Advance a tick's bookkeeping for one that is landing now.
    pub(crate) fn take_tick(&mut self, r: AuraRef, uid: u32) -> Option<u32> {
        let now = self.now;
        let i = self.find_aura(r)?;
        let a = self.actor_mut(r.holder)?;
        let meta = &mut a.meta[i];
        if meta.uid != uid {
            return None;
        }
        let tick = meta.tick.as_mut()?;
        if tick.next_at != Some(now) {
            return None;
        }
        tick.index += 1;
        tick.last_at = now;
        tick.next_at = None;
        Some(tick.index)
    }

    /// After a tick resolves: re-derive the period at the source's current
    /// haste and queue the next one.
    pub(crate) fn continue_ticking(&mut self, r: AuraRef, uid: u32) {
        let s = Arc::clone(&self.s);
        let Some(p) = s
            .setup
            .data
            .auras
            .get(&r.aura)
            .and_then(|d| d.periodic.as_ref())
        else {
            return;
        };
        let haste = self.source_haste(r);
        let Some(i) = self.find_aura(r) else { return };
        let Some(a) = self.actor_mut(r.holder) else {
            return;
        };
        if a.meta[i].uid != uid {
            return;
        }
        if let Some(tick) = &mut a.meta[i].tick {
            tick.period = period(p, haste);
        }
        self.schedule_aura(r);
    }

    /// The final tick owed at expiry: `(index, fraction)`, if any.
    pub(crate) fn final_tick(&mut self, r: AuraRef, uid: u32) -> Option<(u32, f64)> {
        let s = Arc::clone(&self.s);
        let partial = s
            .setup
            .data
            .auras
            .get(&r.aura)
            .and_then(|d| d.periodic.as_ref())
            .is_some_and(|p| p.partial_final_tick);
        let now = self.now;
        let i = self.find_aura(r)?;
        let a = self.actor_mut(r.holder)?;
        if a.meta[i].uid != uid || a.auras[i].expires != Some(now) {
            return None;
        }
        let tick = a.meta[i].tick.as_mut()?;
        let elapsed = f64::from(now.saturating_since(tick.last_at).millis());
        let fraction = elapsed / f64::from(tick.period.millis());
        let full = fraction >= 1.0 - 1e-9;
        if fraction <= 0.0 || !(partial || full) {
            return None;
        }
        tick.index += 1;
        tick.last_at = now;
        Some((tick.index, fraction.min(1.0)))
    }

    pub(crate) fn expire(&mut self, r: AuraRef, uid: u32) {
        let now = self.now;
        let Some(i) = self.find_aura(r) else { return };
        let Some(a) = self.actor_ref(r.holder) else {
            return;
        };
        if a.meta[i].uid == uid && a.auras[i].expires == Some(now) {
            self.remove_instance(r.holder, i, AuraRemoval::Expired);
        }
    }
}
