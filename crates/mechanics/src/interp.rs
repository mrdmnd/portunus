//! Runs data effects and listeners.

use std::sync::Arc;

use portunus_core::{ActorId, AuraId, PetId, SimDuration, SimTime, SpellId, StreamKey};
use portunus_engine::mechanics::{
    whole_points, AuraApplication, DamageEvent, HealEvent, HitKind, RolledHit, ValueLimits,
};
use portunus_engine::state::ActorKind;
use portunus_engine::{AuraRef, EngineIo, ListenerRef, ProcView, SegmentView, StateView};
use portunus_gamedata::effect::{
    AoeRule, Coefficient, CountScale, Effect, EffectTarget, ListenFor, ProcChance, RemovalReason,
    TargetCount,
};
use portunus_gamedata::item::WeaponHand;
use portunus_gamedata::stats::{ResourceKind, SchoolMask};

use crate::math::{instance, owner_seat, predicate, Formulas};
use crate::{
    CombatMath, EffectCtx, EffectInterpreter, IncomingHit, KitTools, Outgoing, SpecRegistry,
};

/// Crit rolls, per caster.
const CRIT_STREAM: StreamKey = StreamKey(0);
/// `Effect::RandomOf` branch choices, per caster.
const RANDOM_STREAM: StreamKey = StreamKey(1);
/// Auto-attack miss rolls, per attacker.
const MISS_STREAM: StreamKey = StreamKey(2);
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

/// One `Effect::Damage`'s fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DamageSpec {
    pub amount: Coefficient,
    pub school: SchoolMask,
    pub target: EffectTarget,
    pub aoe: Option<AoeRule>,
    pub ignores_armor: bool,
    pub hand: Option<WeaponHand>,
    pub per_count: Option<CountScale>,
    pub unmodified: bool,
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
        killing_blow: bool,
    },
    DamageTaken,
    Swing(WeaponHand),
    WeaponHit(WeaponHand),
    PeriodicTick(AuraId),
    ResourceSpent(ResourceKind),
    AuraApplied(AuraId),
    AuraExpired(AuraId),
    PetExpired(PetId),
    /// The holder, a pet, reached the end of its lifetime.
    Departed,
    /// The occurrence's target, an enemy, died.
    EnemyDied {
        /// The listeners' holder dealt the killing blow.
        by_holder: bool,
    },
    CastStart {
        spell: SpellId,
        school: SchoolMask,
    },
    ResourceGained(ResourceKind),
    Absorbed(AuraId),
    Interrupted,
    MoveStart,
    MoveEnd,
    /// The holder's health as fractions of its maximum, before and after
    /// a hit.
    HealthDropped {
        from: f64,
        to: f64,
    },
    AuraStacks {
        aura: AuraId,
        from: u8,
        to: u8,
    },
    AuraRemoved {
        aura: AuraId,
        reason: RemovalReason,
    },
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
                let died_with = |aura: AuraId| {
                    ev.target.is_some_and(|t| {
                        io.view()
                            .auras_at_death(t)
                            .iter()
                            .any(|i| i.aura == aura && i.source == holder)
                    })
                };
                if !listens(l.on, ev.what, died_with) {
                    continue;
                }
                // An earlier listener's effects may have removed the aura.
                if instance(io.view(), r).is_none() {
                    break;
                }
                let listener = ListenerRef {
                    aura: r,
                    index: l
                        .shared_with
                        .unwrap_or_else(|| u8::try_from(index).unwrap_or(u8::MAX)),
                };
                let now = io.view().now();
                let proc_view = |io: &dyn EngineIo| {
                    io.view()
                        .procs(holder)
                        .iter()
                        .find(|p| p.listener == listener)
                        .copied()
                };
                if proc_view(io)
                    .and_then(|p| p.icd_ready_at)
                    .is_some_and(|ready| ready > now)
                {
                    continue;
                }
                if l.condition
                    .is_some_and(|p| !predicate(io.view(), p, holder, ev.target, None))
                {
                    continue;
                }
                let per_unit = l.per_unit && matches!(ev.what, Happening::ResourceSpent(_));
                let attempts = if per_unit {
                    ev.amount.map_or(0, |a| a.max(0.0).round() as u32)
                } else {
                    1
                };
                let mut procced = false;
                for _ in 0..attempts {
                    let hit = match l.chance {
                        ProcChance::Always => true,
                        chance => {
                            let p = self.chance(io.view(), holder, chance, proc_view(io), now);
                            io.roll(holder, proc_stream(listener)) < p
                        }
                    };
                    io.record_proc_attempt(listener, hit);
                    procced |= hit;
                }
                if procced {
                    let scale = match ev.what {
                        Happening::ResourceSpent(_) if !per_unit => ev.amount.unwrap_or(1.0),
                        _ => 1.0,
                    };
                    let hand = match ev.what {
                        Happening::Swing(h) | Happening::WeaponHit(h) => Some(h),
                        _ => None,
                    };
                    let ctx = EffectCtx {
                        caster: holder,
                        target: ev.target,
                        spell: None,
                        aura: Some(r),
                        event_amount: ev.amount,
                        scale,
                        spent: None,
                        depth: 1,
                        hand,
                        hit: HitKind::Direct,
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

    fn targets(
        &self,
        io: &mut dyn EngineIo,
        ctx: &EffectCtx,
        target: EffectTarget,
    ) -> Vec<ActorId> {
        if target == EffectTarget::RandomEnemy {
            let engaged = engaged_enemies(io.view());
            if engaged.is_empty() {
                return engaged;
            }
            let pick = io.roll(ctx.caster, RANDOM_STREAM) * engaged.len() as f64;
            let index = (pick as usize).min(engaged.len() - 1);
            return vec![engaged[index]];
        }
        let view = io.view();
        let alive = |id: &ActorId| view.actor(*id).is_some_and(|a| a.alive);
        let mut out: Vec<ActorId> = match target {
            EffectTarget::Caster => vec![ctx.caster],
            EffectTarget::Target => ctx.target.into_iter().collect(),
            EffectTarget::AllEnemies => engaged_enemies(view),
            EffectTarget::RandomEnemy => Vec::new(),
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
            EffectTarget::EnemiesWithAura { aura, from_self } => engaged_enemies(view)
                .into_iter()
                .filter(|&e| holds_aura(view, e, aura, from_self.then_some(ctx.caster)))
                .collect(),
            EffectTarget::OtherEnemies => engaged_enemies(view)
                .into_iter()
                .filter(|&e| Some(e) != ctx.target)
                .collect(),
            EffectTarget::AlliesWithAura { aura, from_self } => view
                .seats()
                .iter()
                .copied()
                .filter(|&s| holds_aura(view, s, aura, from_self.then_some(ctx.caster)))
                .collect(),
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

    /// One context per target of a `ForEach`, each as the event's target.
    fn each(&self, io: &mut dyn EngineIo, ctx: &EffectCtx, target: EffectTarget) -> Vec<EffectCtx> {
        self.targets(io, ctx, target)
            .into_iter()
            .map(|t| EffectCtx {
                target: Some(t),
                ..*ctx
            })
            .collect()
    }

    pub(crate) fn damage(&self, io: &mut dyn EngineIo, ctx: &EffectCtx, d: DamageSpec) {
        let Some(ctx) = self.striking(io.view(), ctx, d.hand) else {
            return;
        };
        for c in self.damage_targets(io, &ctx, d) {
            let hit = self.roll_hit(io, &c, d);
            self.deliver(io, hit);
        }
    }

    /// Effects run by an aura on its holder, cast by its source: its
    /// threshold effects and limits. Like its ticks, it counts as the
    /// spell of the same id, if there is one.
    pub(crate) fn aura_ctx(&self, r: AuraRef) -> EffectCtx {
        let spell = SpellId(r.aura.0);
        EffectCtx {
            caster: r.source,
            target: Some(r.holder),
            spell: self
                .math
                .data()
                .spells
                .contains_key(&spell)
                .then_some(spell),
            aura: Some(r),
            event_amount: None,
            scale: 1.0,
            spent: None,
            depth: 0,
            hand: None,
            hit: HitKind::Direct,
        }
    }

    /// A valued aura's cap and threshold, scaled by its source (Seed of
    /// Corruption's threshold is the warlock's spell power) and, for
    /// `PctMaxHealth`, its holder.
    pub(crate) fn value_limits(&self, view: &dyn StateView, r: AuraRef) -> ValueLimits {
        let Some(v) = self
            .math
            .data()
            .auras
            .get(&r.aura)
            .and_then(|d| d.value.as_ref())
        else {
            return ValueLimits::default();
        };
        let ctx = self.aura_ctx(r);
        let eval = |c: Option<Coefficient>| c.map(|c| self.math.base(view, &ctx, c));
        ValueLimits {
            cap: eval(v.cap),
            threshold: eval(v.threshold),
        }
    }

    /// The context for a hit that strikes with `hand`, if any: `None` when
    /// there's no weapon in that hand.
    fn striking(
        &self,
        view: &dyn StateView,
        ctx: &EffectCtx,
        hand: Option<WeaponHand>,
    ) -> Option<EffectCtx> {
        let Some(hand) = hand else {
            return Some(*ctx);
        };
        self.math.weapon(view, ctx.caster, hand)?;
        Some(EffectCtx {
            hand: Some(hand),
            ..*ctx
        })
    }

    /// One context per target hit, each scaled by its AoE share and its
    /// count scale.
    fn damage_targets(
        &self,
        io: &mut dyn EngineIo,
        ctx: &EffectCtx,
        d: DamageSpec,
    ) -> Vec<EffectCtx> {
        let aoe = d.aoe;
        let mut targets = self.targets(io, ctx, d.target);
        let mut falloff = 1.0;
        let secondary = aoe.map_or(1.0, |rule| rule.secondary);
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
        if let Some(scale) = d.per_count {
            let count = match scale.count {
                TargetCount::TargetsHit => targets.len(),
                TargetCount::EnemiesWithAura { aura, from_self } => {
                    let view = io.view();
                    engaged_enemies(view)
                        .into_iter()
                        .filter(|&e| holds_aura(view, e, aura, from_self.then_some(ctx.caster)))
                        .count()
                }
            };
            falloff *= 1.0 + scale.pct / 100.0 * count as f64;
        }
        targets
            .into_iter()
            .map(|t| {
                let share = if ctx.target == Some(t) {
                    1.0
                } else {
                    secondary
                };
                EffectCtx {
                    target: Some(t),
                    scale: ctx.scale * falloff * share,
                    ..*ctx
                }
            })
            .collect()
    }

    /// Add to the caster's resource; on a gain, fire its `ResourceGained`
    /// listeners with what it got after the cap.
    fn gain(&self, io: &mut dyn EngineIo, ctx: &EffectCtx, kind: ResourceKind, amount: f64) {
        let value = |io: &dyn EngineIo| {
            io.view()
                .resource(ctx.caster, kind)
                .map_or(0.0, |r| r.value)
        };
        let before = value(io);
        io.add_resource(ctx.caster, kind, amount);
        let gained = value(io) - before;
        if gained > 0.0 {
            let ev = Occurrence {
                what: Happening::ResourceGained(kind),
                target: ctx.target,
                amount: Some(gained),
                depth: ctx.depth,
            };
            self.fire(io, ctx.caster, ev);
        }
    }

    /// After a hit on `holder` from `attacker`: its `HealthBelow`
    /// listeners, given its health fractions before and after.
    pub(crate) fn health_dropped(
        &self,
        io: &mut dyn EngineIo,
        holder: ActorId,
        (from, to): (Option<f64>, Option<f64>),
        attacker: ActorId,
        depth: u8,
    ) {
        let (Some(from), Some(to)) = (from, to) else {
            return;
        };
        if to < from {
            let ev = Occurrence {
                what: Happening::HealthDropped { from, to },
                target: Some(attacker),
                amount: None,
                depth,
            };
            self.fire(io, holder, ev);
        }
    }

    /// An auto-attack by `actor` with `hand` at `target`: miss, else a
    /// weapon-damage hit, then the attacker's `Swing` and `WeaponHit`
    /// listeners. The miss roll is drawn whatever the chance, so changing
    /// it never shifts later rolls.
    pub(crate) fn white_hit(
        &self,
        io: &mut dyn EngineIo,
        actor: ActorId,
        hand: WeaponHand,
        target: ActorId,
        depth: u8,
    ) {
        let miss = self.math.white_miss_chance(io.view(), actor);
        if io.roll(actor, MISS_STREAM) < miss {
            return;
        }
        let raw = self.math.weapon_damage(io.view(), actor, hand);
        let ctx = EffectCtx {
            caster: actor,
            target: Some(target),
            spell: None,
            aura: None,
            event_amount: None,
            scale: 1.0,
            spent: None,
            depth,
            hand: Some(hand),
            hit: HitKind::Direct,
        };
        self.damage(
            io,
            &ctx,
            DamageSpec {
                amount: Coefficient::Flat(raw),
                school: SchoolMask::PHYSICAL,
                target: EffectTarget::Target,
                aoe: None,
                ignores_armor: false,
                hand: None,
                per_count: None,
                unmodified: false,
            },
        );
        for what in [Happening::Swing(hand), Happening::WeaponHit(hand)] {
            let ev = Occurrence {
                what,
                target: Some(target),
                amount: None,
                depth,
            };
            self.fire(io, actor, ev);
        }
    }

    /// Amount and crit against `c.target`, before its mitigation.
    fn roll_hit(&self, io: &mut dyn EngineIo, c: &EffectCtx, d: DamageSpec) -> RolledHit {
        let view = io.view();
        let (raw, chance) = if d.unmodified {
            (self.math.base(view, c, d.amount), 0.0)
        } else {
            (
                self.math
                    .outgoing(view, c, d.amount, Outgoing::Damage(d.school)),
                self.math.crit_chance(view, c),
            )
        };
        let mult = self.math.crit_multiplier(view, c);
        let crit = io.roll(c.caster, CRIT_STREAM) < chance;
        let weapon = d.hand.or_else(|| {
            c.spell
                .and_then(|s| self.math.data().spells.get(&s))
                .and_then(|def| def.weapon)
        });
        RolledHit {
            source: c.caster,
            target: c.target.unwrap_or(c.caster),
            amount: if crit { raw * mult } else { raw },
            school: d.school,
            spell: c.spell,
            crit,
            depth: c.depth,
            kind: c.hit,
            ignores_armor: d.ignores_armor,
            weapon,
        }
    }

    /// Mitigate, apply, and run the listeners for a hit.
    fn deliver(&self, io: &mut dyn EngineIo, hit: RolledHit) {
        let t = hit.target;
        let alive = |io: &dyn EngineIo| io.view().actor(t).is_some_and(|a| a.alive);
        let was_alive = alive(io);
        let health_before = health_fraction(io.view(), t);
        let amount = self
            .math
            .mitigate(io.view(), t, hit.amount, IncomingHit::of(&hit));
        let landed = io.apply_damage(DamageEvent {
            source: hit.source,
            target: t,
            amount: whole_points(amount),
            school: hit.school,
            spell: hit.spell,
            crit: hit.crit,
        });
        let health = (health_before, health_fraction(io.view(), t));
        if landed.connected() {
            let dealt = Occurrence {
                what: Happening::DamageDealt {
                    spell: hit.spell,
                    school: hit.school,
                    crit: hit.crit,
                    killing_blow: was_alive && !alive(io),
                },
                target: Some(t),
                amount: Some(landed.amount as f64),
                depth: hit.depth,
            };
            self.fire(io, hit.source, dealt);
            let taken = Occurrence {
                what: Happening::DamageTaken,
                target: Some(hit.source),
                ..dealt
            };
            self.fire(io, t, taken);
            self.health_dropped(io, t, health, hit.source, hit.depth);
            if let Some(hand) = hit.weapon {
                let struck = Occurrence {
                    what: Happening::WeaponHit(hand),
                    target: Some(t),
                    amount: Some(landed.amount as f64),
                    depth: hit.depth,
                };
                self.fire(io, hit.source, struck);
            }
        }
    }

    /// A travelling spell's direct damage, rolled now as SimC does at
    /// execute and delivered when it lands. Conditions on damage are read
    /// now too.
    pub(crate) fn roll_direct(
        &self,
        io: &mut dyn EngineIo,
        ctx: &EffectCtx,
        effects: &[Effect],
    ) -> Vec<RolledHit> {
        let mut hits = Vec::new();
        for effect in effects {
            match effect {
                &Effect::Damage {
                    amount,
                    school,
                    target,
                    aoe,
                    ignores_armor,
                    hand,
                    per_count,
                    unmodified,
                } => {
                    let d = DamageSpec {
                        amount,
                        school,
                        target,
                        aoe,
                        ignores_armor,
                        hand,
                        per_count,
                        unmodified,
                    };
                    let Some(ctx) = self.striking(io.view(), ctx, hand) else {
                        continue;
                    };
                    for c in self.damage_targets(io, &ctx, d) {
                        hits.push(self.roll_hit(io, &c, d));
                    }
                }
                Effect::ForEach { target, then } => {
                    for c in self.each(io, ctx, *target) {
                        hits.extend(self.roll_direct(io, &c, then));
                    }
                }
                Effect::If {
                    when,
                    then,
                    otherwise,
                } => {
                    let holds = predicate(io.view(), *when, ctx.caster, ctx.target, ctx.spell);
                    hits.extend(self.roll_direct(io, ctx, if holds { then } else { otherwise }));
                }
                _ => {}
            }
        }
        hits
    }

    /// A triggered spell that travels or goes out later, rolled at once
    /// unless it rolls on impact: an overload snapshots its parent's buffs
    /// before the parent's own listeners spend them.
    fn roll_ahead(
        &self,
        io: &mut dyn EngineIo,
        caster: ActorId,
        spell: SpellId,
        target: Option<ActorId>,
        delay: SimDuration,
    ) -> Option<Vec<RolledHit>> {
        let def = self.math.data().spells.get(&spell)?;
        if !(def.travels() || delay > SimDuration::ZERO) || def.rolls_on_impact {
            return None;
        }
        let ctx = EffectCtx {
            caster,
            target,
            spell: Some(spell),
            aura: None,
            event_amount: None,
            scale: 1.0,
            spent: None,
            depth: 0,
            hand: def.weapon,
            hit: HitKind::Direct,
        };
        Some(self.roll_direct(io, &ctx, &def.effects))
    }

    /// Its landing: deliver the hits rolled at launch, then run everything
    /// else (resources, auras) as it lands.
    pub(crate) fn land(
        &self,
        io: &mut dyn EngineIo,
        ctx: &EffectCtx,
        effects: &[Effect],
        hits: &[RolledHit],
    ) {
        for &hit in hits {
            self.deliver(io, hit);
        }
        self.run_besides_damage(io, ctx, effects);
    }

    fn run_besides_damage(&self, io: &mut dyn EngineIo, ctx: &EffectCtx, effects: &[Effect]) {
        for effect in effects {
            match effect {
                Effect::Damage { .. } => {}
                Effect::If {
                    when,
                    then,
                    otherwise,
                } => {
                    let holds = predicate(io.view(), *when, ctx.caster, ctx.target, ctx.spell);
                    self.run_besides_damage(io, ctx, if holds { then } else { otherwise });
                }
                Effect::ForEach { target, then } => {
                    for c in self.each(io, ctx, *target) {
                        self.run_besides_damage(io, &c, then);
                    }
                }
                other => self.run_one(io, ctx, other),
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
        for t in self.targets(io, ctx, target) {
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
                ignores_armor,
                hand,
                per_count,
                unmodified,
            } => self.damage(
                io,
                ctx,
                DamageSpec {
                    amount,
                    school,
                    target,
                    aoe,
                    ignores_armor,
                    hand,
                    per_count,
                    unmodified,
                },
            ),
            &Effect::Heal { amount, target } => self.heal(io, ctx, amount, target),
            &Effect::ApplyAura {
                aura,
                target,
                stacks,
                duration,
                per_unit_spent,
            } => {
                let duration = match per_unit_spent {
                    Some(per) => duration
                        .or_else(|| io.data().auras.get(&aura).and_then(|a| a.duration))
                        .map(|base| {
                            let extra = f64::from(per.millis()) * ctx.spent.unwrap_or(0.0);
                            SimDuration(base.millis().saturating_add(extra.round() as u32))
                        }),
                    None => duration,
                };
                for t in self.targets(io, ctx, target) {
                    io.apply_aura(AuraApplication {
                        aura: aura_on(t, aura),
                        stacks,
                        duration,
                        pmultiplier: None,
                    });
                }
            }
            &Effect::RemoveAura { aura, target } => {
                for t in self.targets(io, ctx, target) {
                    io.remove_aura(t, aura, Some(caster));
                }
            }
            &Effect::RemoveStacks {
                aura,
                target,
                stacks,
            } => {
                for t in self.targets(io, ctx, target) {
                    io.remove_stacks(aura_on(t, aura), stacks);
                }
            }
            &Effect::ExtendAura { aura, target, by } => {
                for t in self.targets(io, ctx, target) {
                    io.extend_aura(aura_on(t, aura), by);
                }
            }
            &Effect::AddAuraValue {
                aura,
                target,
                amount,
            } => {
                for t in self.targets(io, ctx, target) {
                    let c = EffectCtx {
                        target: Some(t),
                        ..*ctx
                    };
                    let delta = self.math.base(io.view(), &c, amount);
                    let r = own_or(ctx, aura_on(t, aura));
                    let limits = self.value_limits(io.view(), r);
                    io.add_aura_value(r, delta, limits);
                }
            }
            &Effect::ConsumeAuraValue {
                aura,
                target,
                fraction,
            } => {
                for t in self.targets(io, ctx, target) {
                    let r = own_or(ctx, aura_on(t, aura));
                    if let Some(value) = instance(io.view(), r).map(|i| i.value) {
                        let limits = self.value_limits(io.view(), r);
                        io.add_aura_value(r, -value * fraction, limits);
                    }
                }
            }
            &Effect::Resource(r) => self.gain(io, ctx, r.kind, r.amount * ctx.scale),
            &Effect::GainResource { kind, amount } => {
                let gain = self.math.base(io.view(), ctx, amount);
                self.gain(io, ctx, kind, gain);
            }
            Effect::ExtraSwing => {
                let hand = ctx.hand.unwrap_or(WeaponHand::MainHand);
                let target = ctx.target.or_else(|| io.view().target(caster));
                let armed = self.math.weapon(io.view(), caster, hand).is_some();
                if let (true, Some(target)) = (armed, target) {
                    self.white_hit(io, caster, hand, target, ctx.depth);
                }
            }
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
            &Effect::TriggerSpell {
                spell,
                target,
                delay,
            } => {
                let target = self.targets(io, ctx, target).first().copied();
                let rolled = self.roll_ahead(io, caster, spell, target, delay);
                io.trigger_spell(caster, spell, target, delay, rolled);
            }
            &Effect::Interrupt { target } => {
                for t in self.targets(io, ctx, target) {
                    if io.interrupt(t) {
                        let stopped = Occurrence {
                            what: Happening::Interrupted,
                            target: Some(t),
                            amount: None,
                            depth: ctx.depth,
                        };
                        self.fire(io, caster, stopped);
                    }
                }
            }
            &Effect::Displace {
                yards,
                toward_target,
            } => io.displace(caster, yards, ctx.target.filter(|_| toward_target)),
            Effect::ApplyOneOf { auras, target } => {
                for t in self.targets(io, ctx, *target) {
                    let held = |a: &AuraId| io.view().auras(t).iter().any(|i| i.aura == *a);
                    let lacking: Vec<AuraId> = auras.iter().copied().filter(|a| !held(a)).collect();
                    let pool = if lacking.is_empty() {
                        auras.clone()
                    } else {
                        lacking
                    };
                    if pool.is_empty() {
                        continue;
                    }
                    let pick = io.roll(caster, RANDOM_STREAM) * pool.len() as f64;
                    let aura = pool[(pick as usize).min(pool.len() - 1)];
                    io.apply_aura(AuraApplication {
                        aura: aura_on(t, aura),
                        stacks: 1,
                        duration: None,
                        pmultiplier: None,
                    });
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
            Effect::Chance { chance, then } => {
                if io.roll(caster, RANDOM_STREAM) < *chance {
                    self.run(io, ctx, then);
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
            Effect::ForEach { target, then } => {
                for c in self.each(io, ctx, *target) {
                    self.run(io, &c, then);
                }
            }
            Effect::Hook(key) => {
                if let Some(kit) = self.kits.hook_owner(key) {
                    kit.run_hook(self.tools(), io, ctx, key);
                }
            }
            &Effect::SpreadAura { aura, to, max } => {
                let Some(from) = ctx.target else { return };
                let view = io.view();
                let Some(&copy) = instance(view, aura_on(from, aura)) else {
                    return;
                };
                let duration = copy.expires.map(|e| e.saturating_since(view.now()));
                if duration == Some(SimDuration::ZERO) {
                    return;
                }
                let lacking: Vec<ActorId> = self
                    .targets(io, ctx, to)
                    .into_iter()
                    .filter(|&t| t != from && !holds_aura(io.view(), t, aura, Some(caster)))
                    .take(max.map_or(usize::MAX, usize::from))
                    .collect();
                for t in lacking {
                    io.apply_aura(AuraApplication {
                        aura: aura_on(t, aura),
                        stacks: copy.stacks,
                        duration,
                        pmultiplier: Some(copy.pmultiplier),
                    });
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

/// The aura running the effect, if `r` names it on its own holder (Seed of
/// Corruption's debuff counting the damage its holder takes, whoever the
/// listener runs as), else `r`.
fn own_or(ctx: &EffectCtx, r: AuraRef) -> AuraRef {
    match ctx.aura {
        Some(own) if own.aura == r.aura && own.holder == r.holder => own,
        _ => r,
    }
}

/// Whether `on` hears `what`. `died_with` says whether the dead enemy of
/// an `EnemyDied` held an aura from the listener's holder.
fn listens(on: ListenFor, what: Happening, died_with: impl Fn(AuraId) -> bool) -> bool {
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
                killing_blow,
            },
            Happening::DamageDealt {
                spell: got,
                school: got_school,
                crit,
                killing_blow: fatal,
            },
        ) => {
            spell.is_none_or(|s| got == Some(s))
                && school_ok(school, got_school)
                && (crit || !crit_only)
                && (fatal || !killing_blow)
        }
        (ListenFor::DamageTaken, Happening::DamageTaken) => true,
        (ListenFor::Swing { hand }, Happening::Swing(got))
        | (ListenFor::WeaponHit { hand }, Happening::WeaponHit(got)) => {
            hand.is_none_or(|h| h == got)
        }
        (ListenFor::PeriodicTick(a), Happening::PeriodicTick(b))
        | (ListenFor::AuraApplied(a), Happening::AuraApplied(b))
        | (ListenFor::AuraExpired(a), Happening::AuraExpired(b)) => a == b,
        (ListenFor::ResourceSpent(a), Happening::ResourceSpent(b)) => a == b,
        (ListenFor::PetExpired(a), Happening::PetExpired(b)) => a == b,
        (ListenFor::Departed, Happening::Departed) => true,
        (
            ListenFor::EnemyDied {
                killed_by_self,
                had_aura,
            },
            Happening::EnemyDied { by_holder },
        ) => (by_holder || !killed_by_self) && had_aura.is_none_or(died_with),
        (
            ListenFor::CastStart { spell, school },
            Happening::CastStart {
                spell: got,
                school: got_school,
            },
        ) => spell.is_none_or(|s| s == got) && school_ok(school, got_school),
        (ListenFor::ResourceGained(a), Happening::ResourceGained(b)) => a == b,
        (ListenFor::Absorbed(a), Happening::Absorbed(b)) => a == b,
        (ListenFor::Interrupted, Happening::Interrupted)
        | (ListenFor::MoveStart, Happening::MoveStart)
        | (ListenFor::MoveEnd, Happening::MoveEnd) => true,
        (ListenFor::HealthBelow { pct }, Happening::HealthDropped { from, to }) => {
            let line = f64::from(pct) / 100.0;
            from >= line && to < line
        }
        (
            ListenFor::AuraStacksReached { aura, stacks },
            Happening::AuraStacks {
                aura: got,
                from,
                to,
            },
        ) => aura == got && from < stacks && to >= stacks,
        (
            ListenFor::AuraRemovedBy { aura, reason },
            Happening::AuraRemoved {
                aura: got,
                reason: why,
            },
        ) => aura == got && reason == why,
        _ => false,
    }
}

/// `holder` has `aura`, from `source` if given.
fn holds_aura(
    view: &dyn StateView,
    holder: ActorId,
    aura: AuraId,
    source: Option<ActorId>,
) -> bool {
    view.auras(holder)
        .iter()
        .any(|i| i.aura == aura && source.is_none_or(|s| i.source == s))
}

/// Alive, with a health pool: its health over its maximum.
pub(crate) fn health_fraction(view: &dyn StateView, actor: ActorId) -> Option<f64> {
    view.actor(actor)
        .filter(|a| a.alive && a.max_health > 0)
        .map(|a| a.health as f64 / a.max_health as f64)
}

fn engaged_enemies(view: &dyn StateView) -> Vec<ActorId> {
    view.enemies()
        .iter()
        .copied()
        .filter(|&e| view.actor(e).is_some_and(|a| a.engaged && a.alive))
        .collect()
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
