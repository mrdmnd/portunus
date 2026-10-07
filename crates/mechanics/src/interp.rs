//! Runs data effects and listeners.

use std::sync::Arc;

use portunus_core::{ActorId, AuraId, SimTime, SpellId, StreamKey};
use portunus_engine::mechanics::{whole_points, AuraApplication, DamageEvent, HealEvent};
use portunus_engine::state::ActorKind;
use portunus_engine::{AuraRef, EngineIo, ListenerRef, ProcView, SegmentView, StateView};
use portunus_gamedata::effect::{
    AoeRule, Coefficient, Effect, EffectTarget, ListenFor, ProcChance,
};
use portunus_gamedata::item::WeaponHand;
use portunus_gamedata::stats::{ResourceKind, SchoolMask};

use crate::math::{instance, owner_seat, predicate, Formulas};
use crate::{CombatMath, EffectCtx, EffectInterpreter, KitTools, Outgoing, SpecRegistry};

/// Crit rolls, per caster.
const CRIT_STREAM: StreamKey = StreamKey(0);
/// `Effect::RandomOf` branch choices, per caster.
const RANDOM_STREAM: StreamKey = StreamKey(1);
/// Listener streams are numbered from here, one per listener.
const PROC_STREAM_BASE: u16 = 16;

/// RPPM chance stops growing this long after the previous attempt.
const RPPM_MAX_GAP_SECS: f64 = 3.5;

/// The [`EffectInterpreter`] implementation, plus listener dispatch.
///
/// Each damage or heal rolls crit from the caster's crit stream whatever its
/// chance, so changing a crit chance never shifts later rolls.
#[derive(Clone)]
pub struct Interpreter {
    math: Formulas,
    kits: Arc<dyn SpecRegistry>,
}

/// Something listeners may react to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Happening {
    CastComplete {
        spell: SpellId,
        school: SchoolMask,
    },
    DamageDealt {
        spell: Option<SpellId>,
        school: SchoolMask,
        crit: bool,
    },
    DamageTaken,
    Swing(WeaponHand),
    PeriodicTick(AuraId),
    ResourceSpent(ResourceKind),
    AuraApplied(AuraId),
    AuraExpired(AuraId),
}

/// A happening, as seen by the listeners of one holder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Occurrence {
    pub what: Happening,
    /// What a listener's `EffectTarget::Target` means.
    pub target: Option<ActorId>,
    pub amount: Option<f64>,
    /// The depth of the effects that caused it; only depth 0 fires.
    pub depth: u8,
}

impl Interpreter {
    pub fn new(math: Formulas, kits: Arc<dyn SpecRegistry>) -> Self {
        Self { math, kits }
    }

    pub fn math(&self) -> &Formulas {
        &self.math
    }

    pub(crate) fn kits(&self) -> &dyn SpecRegistry {
        &*self.kits
    }

    pub(crate) fn tools(&self) -> KitTools<'_> {
        KitTools {
            effects: self,
            math: &self.math,
        }
    }

    /// Run the listeners on `holder`'s auras that match `ev`.
    pub(crate) fn fire(&self, io: &mut dyn EngineIo, holder: ActorId, ev: Occurrence) {
        if ev.depth > 0 {
            return;
        }
        let data = self.math.data();
        let held: Vec<AuraRef> = io
            .view()
            .auras(holder)
            .iter()
            .map(|i| AuraRef {
                holder,
                aura: i.aura,
                source: i.source,
            })
            .collect();
        for r in held {
            let Some(def) = data.auras.get(&r.aura) else {
                continue;
            };
            for (index, l) in def.listeners.iter().enumerate() {
                if !listens(l.on, ev.what) {
                    continue;
                }
                // An earlier listener's effects may have removed the aura.
                if instance(io.view(), r).is_none() {
                    break;
                }
                let listener = ListenerRef {
                    aura: r,
                    index: u8::try_from(index).unwrap_or(u8::MAX),
                };
                let now = io.view().now();
                let proc = io
                    .view()
                    .procs(holder)
                    .iter()
                    .find(|p| p.listener == listener)
                    .copied();
                if proc
                    .and_then(|p| p.icd_ready_at)
                    .is_some_and(|ready| ready > now)
                {
                    continue;
                }
                let procced = match l.chance {
                    ProcChance::Always => true,
                    chance => {
                        let p = self.chance(io.view(), holder, chance, proc, now);
                        io.roll(holder, proc_stream(listener)) < p
                    }
                };
                io.record_proc_attempt(listener, procced);
                if procced {
                    let scale = match ev.what {
                        Happening::ResourceSpent(_) => ev.amount.unwrap_or(1.0),
                        _ => 1.0,
                    };
                    let ctx = EffectCtx {
                        caster: holder,
                        target: ev.target,
                        spell: None,
                        aura: Some(r),
                        event_amount: ev.amount,
                        scale,
                        depth: 1,
                    };
                    self.run(io, &ctx, &l.effects);
                }
            }
        }
    }

    /// A listener's chance of proccing now, in `[0, 1]` or above for
    /// certainty.
    fn chance(
        &self,
        view: &dyn StateView,
        holder: ActorId,
        chance: ProcChance,
        proc: Option<ProcView>,
        now: SimTime,
    ) -> f64 {
        match chance {
            ProcChance::Always => 1.0,
            ProcChance::Flat(p) => p,
            ProcChance::StatScaled { stat, coef } => {
                coef * self.math.rated_pct(view, holder, stat) / 100.0
            }
            ProcChance::Rppm { rate, hasted } => {
                let haste = if hasted {
                    self.math.haste_mult(view, holder)
                } else {
                    1.0
                };
                let secs = |t: SimTime| f64::from(now.saturating_since(t).millis()) / 1000.0;
                let gap = proc
                    .and_then(|p| p.last_attempt)
                    .map_or(RPPM_MAX_GAP_SECS, secs)
                    .min(RPPM_MAX_GAP_SECS);
                let combat_start = match view.segment() {
                    SegmentView::Combat(c) => c.started,
                    SegmentView::Travel { .. } | SegmentView::Finished => now,
                };
                let dry = secs(proc.and_then(|p| p.last_proc).unwrap_or(combat_start));
                let bad_luck = (1.0 + (dry / (60.0 / rate) - 1.5) * 3.0).max(1.0);
                rate * haste * gap / 60.0 * bad_luck
            }
            ProcChance::Deck { successes, size } => {
                let left = proc
                    .and_then(|p| p.deck)
                    .map_or((f64::from(successes), f64::from(size)), |d| {
                        (f64::from(d.successes), f64::from(d.cards))
                    });
                if left.1 > 0.0 {
                    left.0 / left.1
                } else {
                    0.0
                }
            }
        }
    }

    fn targets(&self, view: &dyn StateView, ctx: &EffectCtx, target: EffectTarget) -> Vec<ActorId> {
        let alive = |id: &ActorId| view.actor(*id).is_some_and(|a| a.alive);
        let mut out: Vec<ActorId> = match target {
            EffectTarget::Caster => vec![ctx.caster],
            EffectTarget::Target => ctx.target.into_iter().collect(),
            EffectTarget::AllEnemies => view
                .enemies()
                .iter()
                .copied()
                .filter(|&e| view.actor(e).is_some_and(|a| a.engaged))
                .collect(),
            EffectTarget::Party => view.seats().to_vec(),
            EffectTarget::Pets(kind) => owner_seat(view, ctx.caster)
                .map(|seat| {
                    view.pets(seat)
                        .iter()
                        .copied()
                        .filter(|&p| {
                            kind.is_none_or(|want| {
                                matches!(
                                    view.actor(p).map(|a| a.kind),
                                    Some(ActorKind::Pet { pet, .. }) if pet == want
                                )
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            EffectTarget::Owner => match view.actor(ctx.caster).map(|a| a.kind) {
                Some(ActorKind::Pet { owner, .. }) => view
                    .seats()
                    .get(usize::from(owner.0))
                    .copied()
                    .into_iter()
                    .collect(),
                _ => Vec::new(),
            },
        };
        out.retain(alive);
        out
    }

    pub(crate) fn damage(
        &self,
        io: &mut dyn EngineIo,
        ctx: &EffectCtx,
        amount: Coefficient,
        school: SchoolMask,
        target: EffectTarget,
        aoe: Option<AoeRule>,
    ) {
        let mut targets = self.targets(io.view(), ctx, target);
        let mut falloff = 1.0;
        if let Some(rule) = aoe {
            if let Some(primary) = ctx
                .target
                .and_then(|p| targets.iter().position(|&t| t == p))
            {
                targets[..=primary].rotate_right(1);
            }
            if let Some(max) = rule.max_targets {
                targets.truncate(usize::from(max));
            }
            if let Some(cap) = rule.sqrt_cap {
                let (n, cap) = (targets.len() as f64, f64::from(cap));
                if n > cap {
                    falloff = (cap / n).sqrt();
                }
            }
        }
        for t in targets {
            let c = EffectCtx {
                target: Some(t),
                scale: ctx.scale * falloff,
                ..*ctx
            };
            let view = io.view();
            let raw = self
                .math
                .outgoing(view, &c, amount, Outgoing::Damage(school));
            let chance = self.math.crit_chance(view, &c);
            let mult = self.math.crit_multiplier(view, &c);
            let crit = io.roll(ctx.caster, CRIT_STREAM) < chance;
            let hit = if crit { raw * mult } else { raw };
            let hit = self.math.mitigate(io.view(), t, hit, school);
            let landed = io.apply_damage(DamageEvent {
                source: ctx.caster,
                target: t,
                amount: whole_points(hit),
                school,
                spell: ctx.spell,
                crit,
            });
            if landed > 0 {
                let dealt = Occurrence {
                    what: Happening::DamageDealt {
                        spell: ctx.spell,
                        school,
                        crit,
                    },
                    target: Some(t),
                    amount: Some(landed as f64),
                    depth: ctx.depth,
                };
                self.fire(io, ctx.caster, dealt);
                let taken = Occurrence {
                    what: Happening::DamageTaken,
                    target: Some(ctx.caster),
                    ..dealt
                };
                self.fire(io, t, taken);
            }
        }
    }

    fn heal(
        &self,
        io: &mut dyn EngineIo,
        ctx: &EffectCtx,
        amount: Coefficient,
        target: EffectTarget,
    ) {
        for t in self.targets(io.view(), ctx, target) {
            let c = EffectCtx {
                target: Some(t),
                ..*ctx
            };
            let view = io.view();
            let raw = self.math.outgoing(view, &c, amount, Outgoing::Heal);
            let chance = self.math.crit_chance(view, &c);
            let mult = self.math.crit_multiplier(view, &c);
            let crit = io.roll(ctx.caster, CRIT_STREAM) < chance;
            io.apply_heal(HealEvent {
                source: ctx.caster,
                target: t,
                amount: whole_points(if crit { raw * mult } else { raw }),
                spell: ctx.spell,
            });
        }
    }

    fn run_one(&self, io: &mut dyn EngineIo, ctx: &EffectCtx, effect: &Effect) {
        let caster = ctx.caster;
        let aura_on = |holder: ActorId, aura: AuraId| AuraRef {
            holder,
            aura,
            source: caster,
        };
        match effect {
            &Effect::Damage {
                amount,
                school,
                target,
                aoe,
            } => self.damage(io, ctx, amount, school, target, aoe),
            &Effect::Heal { amount, target } => self.heal(io, ctx, amount, target),
            &Effect::ApplyAura {
                aura,
                target,
                stacks,
            } => {
                for t in self.targets(io.view(), ctx, target) {
                    io.apply_aura(AuraApplication {
                        aura: aura_on(t, aura),
                        stacks,
                        duration: None,
                    });
                }
            }
            &Effect::RemoveAura { aura, target } => {
                for t in self.targets(io.view(), ctx, target) {
                    io.remove_aura(t, aura, Some(caster));
                }
            }
            &Effect::RemoveStacks {
                aura,
                target,
                stacks,
            } => {
                for t in self.targets(io.view(), ctx, target) {
                    io.remove_stacks(aura_on(t, aura), stacks);
                }
            }
            &Effect::ExtendAura { aura, target, by } => {
                for t in self.targets(io.view(), ctx, target) {
                    io.extend_aura(aura_on(t, aura), by);
                }
            }
            &Effect::AddAuraValue {
                aura,
                target,
                amount,
            } => {
                for t in self.targets(io.view(), ctx, target) {
                    let c = EffectCtx {
                        target: Some(t),
                        ..*ctx
                    };
                    let delta = self.math.base(io.view(), &c, amount);
                    io.add_aura_value(aura_on(t, aura), delta);
                }
            }
            &Effect::ConsumeAuraValue {
                aura,
                target,
                fraction,
            } => {
                for t in self.targets(io.view(), ctx, target) {
                    let r = aura_on(t, aura);
                    if let Some(value) = instance(io.view(), r).map(|i| i.value) {
                        io.add_aura_value(r, -value * fraction);
                    }
                }
            }
            &Effect::Resource(r) => io.add_resource(caster, r.kind, r.amount),
            &Effect::Summon {
                pet,
                count,
                duration,
            } => {
                if let Some(seat) = owner_seat(io.view(), caster) {
                    io.summon(seat, pet, count, duration);
                }
            }
            &Effect::Dismiss { pet, count } => {
                if let Some(seat) = owner_seat(io.view(), caster) {
                    io.dismiss(seat, pet, count);
                }
            }
            &Effect::CommandPet { pet, spell } => {
                let view = io.view();
                let target = ctx.target.or_else(|| view.target(caster));
                if let (Some(seat), Some(target)) = (owner_seat(view, caster), target) {
                    io.command_pets(seat, pet, spell, target);
                }
            }
            &Effect::ExtendPets { pet, by } => {
                if let Some(seat) = owner_seat(io.view(), caster) {
                    io.extend_pets(seat, pet, by);
                }
            }
            &Effect::AdjustCooldown { spell, change } => io.adjust_cooldown(caster, spell, change),
            &Effect::TriggerSpell { spell, target } => {
                let target = self.targets(io.view(), ctx, target).first().copied();
                io.trigger_spell(caster, spell, target);
            }
            &Effect::Interrupt { target } => {
                for t in self.targets(io.view(), ctx, target) {
                    io.interrupt(t);
                }
            }
            Effect::RandomOf(branches) => {
                let total: f64 = branches.iter().map(|(w, _)| w.max(0.0)).sum();
                if total <= 0.0 {
                    return;
                }
                let mut pick = io.roll(caster, RANDOM_STREAM) * total;
                for (weight, branch) in branches {
                    pick -= weight.max(0.0);
                    if pick < 0.0 {
                        self.run_one(io, ctx, branch);
                        return;
                    }
                }
                if let Some((_, last)) = branches.last() {
                    self.run_one(io, ctx, last);
                }
            }
            Effect::If {
                when,
                then,
                otherwise,
            } => {
                let holds = predicate(io.view(), *when, caster, ctx.target, ctx.spell);
                self.run(io, ctx, if holds { then } else { otherwise });
            }
            Effect::Hook(key) => {
                if let Some(kit) = self.kits.hook_owner(key) {
                    kit.run_hook(self.tools(), io, ctx, key);
                }
            }
        }
    }
}

impl EffectInterpreter for Interpreter {
    fn run(&self, io: &mut dyn EngineIo, ctx: &EffectCtx, effects: &[Effect]) {
        for effect in effects {
            self.run_one(io, ctx, effect);
        }
    }
}

fn listens(on: ListenFor, what: Happening) -> bool {
    let school_ok =
        |want: Option<SchoolMask>, got: SchoolMask| want.is_none_or(|w| w.0 & got.0 != 0);
    match (on, what) {
        (
            ListenFor::CastComplete { spell, school },
            Happening::CastComplete {
                spell: got,
                school: got_school,
            },
        ) => spell.is_none_or(|s| s == got) && school_ok(school, got_school),
        (
            ListenFor::DamageDealt {
                spell,
                school,
                crit_only,
            },
            Happening::DamageDealt {
                spell: got,
                school: got_school,
                crit,
            },
        ) => {
            spell.is_none_or(|s| got == Some(s))
                && school_ok(school, got_school)
                && (crit || !crit_only)
        }
        (ListenFor::DamageTaken, Happening::DamageTaken) => true,
        (ListenFor::Swing { hand }, Happening::Swing(got)) => hand.is_none_or(|h| h == got),
        (ListenFor::PeriodicTick(a), Happening::PeriodicTick(b))
        | (ListenFor::AuraApplied(a), Happening::AuraApplied(b))
        | (ListenFor::AuraExpired(a), Happening::AuraExpired(b)) => a == b,
        (ListenFor::ResourceSpent(a), Happening::ResourceSpent(b)) => a == b,
        _ => false,
    }
}

/// A listener's own stream, from its aura and index (FNV-1a).
fn proc_stream(l: ListenerRef) -> StreamKey {
    let mut h: u32 = 0x811c_9dc5;
    for b in l.aura.aura.0.to_le_bytes().into_iter().chain([l.index]) {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    let span = u32::from(u16::MAX - PROC_STREAM_BASE) + 1;
    StreamKey(PROC_STREAM_BASE + u16::try_from(h % span).unwrap_or(0))
}
