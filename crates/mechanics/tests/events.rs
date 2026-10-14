//! What else listeners hear of, and auras that cheat death, on the
//! synthetic fixture: the seat counts what its kit's listeners saw in rage.

mod synthetic;

use portunus_core::{ActorId, AuraId, Dist, EventName, SimDuration, SimTime, SpellId, Trigger};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{Choice, Engine, Kernel, MoveGoal, StateView, Step, Wait};
use portunus_gamedata::aura::{AuraValue, AuraValueKind, PreventDeath};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget, ListenFor, RemovalReason};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemyTarget};
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::stats::{ResourceAmount, ResourceDef, ResourceKind, SchoolMask};
use portunus_mechanics::PartyMechanics;
use synthetic::*;

const HARD: SpellId = SpellId(900_821);
const CHANNEL: SpellId = SpellId(900_822);
const GAIN: SpellId = SpellId(900_823);
const KICK: SpellId = SpellId(900_824);
const STACK: SpellId = SpellId(900_825);
const STACKY: AuraId = AuraId(900_826);
const PURGE: SpellId = SpellId(900_827);
const SHIELD: AuraId = AuraId(900_828);
const SHIELDING: SpellId = SpellId(900_829);
const SAVE: AuraId = AuraId(900_830);
const LOCKOUT: AuraId = AuraId(900_831);

fn gain_rage_by_event() -> Effect {
    Effect::GainResource {
        kind: ResourceKind::Rage,
        amount: Coefficient::EventAmount(1.0),
    }
}

/// The boss hits the seat for `amount` fire every `every`, from `first`
/// after it engages.
fn hits_seat(amount: f64, first: SimDuration, every: SimDuration) -> EnemyRule {
    EnemyRule {
        name: EventName("hit".into()),
        phase: None,
        when: Trigger::Elapsed(first),
        repeat: Some(Dist::Fixed(every)),
        action: EnemyAction::Damage {
            amount: Dist::Fixed(amount),
            school: SchoolMask::FIRE,
            target: EnemyTarget::Tank,
        },
    }
}

#[test]
fn cast_starts_fire_for_hard_casts_only() {
    let mut f = fixture(vec![
        listener(
            ListenFor::CastStart {
                spell: None,
                school: None,
            },
            vec![rage(1.0)],
        ),
        listener(
            ListenFor::CastStart {
                spell: Some(HARD),
                school: None,
            },
            vec![rage(10.0)],
        ),
    ]);
    let mut hard = spell(HARD, Vec::new());
    hard.cast = CastKind::Cast {
        time: SimDuration(1500),
        hasted: false,
    };
    let mut channel = spell(CHANNEL, Vec::new());
    channel.cast = CastKind::Channel {
        duration: SimDuration(2000),
        ticks: 2,
        hasted: false,
        swings: false,
        tick_zero: false,
        on_end: Vec::new(),
    };
    add_spell(&mut f, hard);
    add_spell(&mut f, channel);
    let mut k = f.kernel();
    let script = [(HARD, 0), (STRIKE, 0), (CHANNEL, 0), (HARD, 0)];
    play(&mut k, &script, COMBAT_START + SimDuration(10_000));
    assert_eq!(rage_of(&k), 22.0);
}

#[test]
fn resource_gains_report_what_the_cap_let_through() {
    let mut f = fixture(vec![
        listener(
            ListenFor::ResourceGained(ResourceKind::Energy),
            vec![gain_rage_by_event()],
        ),
        // The rage above comes from a listener, which nothing hears.
        listener(
            ListenFor::ResourceGained(ResourceKind::Rage),
            vec![rage(1000.0)],
        ),
    ]);
    f.template.resources.push(ResourceDef {
        kind: ResourceKind::Energy,
        max: 100.0,
        initial: 0.0,
        regen_per_sec: 0.0,
        regen_hasted: false,
        recharge: None,
        out_of_combat: None,
    });
    let energy = Effect::Resource(ResourceAmount {
        kind: ResourceKind::Energy,
        amount: 60.0,
    });
    add_spell(&mut f, spell(GAIN, vec![energy]));
    let mut k = f.kernel();
    play(
        &mut k,
        &[(GAIN, 0), (GAIN, 0), (GAIN, 0)],
        COMBAT_START + SimDuration(5000),
    );
    // 60, then the 40 left under the cap, then nothing.
    assert_eq!(rage_of(&k), 100.0);
}

#[test]
fn a_shields_caster_hears_what_it_soaked_and_that_it_ran_out() {
    let mut f = fixture(vec![
        listener(ListenFor::Absorbed(SHIELD), vec![gain_rage_by_event()]),
        listener(
            ListenFor::AuraRemovedBy {
                aura: SHIELD,
                reason: RemovalReason::Depleted,
            },
            vec![rage(10_000.0)],
        ),
        listener(
            ListenFor::AuraRemovedBy {
                aura: SHIELD,
                reason: RemovalReason::Expired,
            },
            vec![rage(100_000.0)],
        ),
    ]);
    let mut shield = aura(SHIELD);
    shield.duration = Some(SimDuration(30_000));
    shield.value = Some(AuraValue {
        kind: AuraValueKind::Absorb {
            school: SchoolMask::FIRE,
        },
        initial: Some(Coefficient::Flat(500.0)),
        cap: None,
        threshold: None,
        on_threshold: Vec::new(),
    });
    f.data.auras.insert(SHIELD, shield);
    add_spell(
        &mut f,
        spell(SHIELDING, vec![apply(SHIELD, EffectTarget::Caster)]),
    );
    f.boss(vec![hits_seat(400.0, SimDuration(2000), SimDuration(2000))]);
    let mut k = f.kernel();
    play(&mut k, &[(SHIELDING, 0)], COMBAT_START + SimDuration(7000));
    assert_eq!(rage_of(&k), 500.0 + 10_000.0);
}

#[test]
fn stack_thresholds_fire_on_reaching_them_and_removals_say_how() {
    let mut f = fixture(vec![
        listener(
            ListenFor::AuraStacksReached {
                aura: STACKY,
                stacks: 3,
            },
            vec![rage(1.0)],
        ),
        listener(
            ListenFor::AuraRemovedBy {
                aura: STACKY,
                reason: RemovalReason::Expired,
            },
            vec![rage(10.0)],
        ),
        listener(
            ListenFor::AuraRemovedBy {
                aura: STACKY,
                reason: RemovalReason::Removed,
            },
            vec![rage(100.0)],
        ),
    ]);
    let mut stacky = aura(STACKY);
    stacky.duration = Some(SimDuration(3000));
    stacky.max_stacks = 5;
    f.data.auras.insert(STACKY, stacky);
    let mut two = apply(STACKY, EffectTarget::Caster);
    if let Effect::ApplyAura { stacks, .. } = &mut two {
        *stacks = 2;
    }
    add_spell(&mut f, spell(STACK, vec![two]));
    let purge = Effect::RemoveAura {
        aura: STACKY,
        target: EffectTarget::Caster,
    };
    add_spell(&mut f, spell(PURGE, vec![purge]));
    let mut k = f.kernel();
    // 2, 4 (past 3), 5; removed; 2 again, which runs out.
    let script = [(STACK, 0), (STACK, 0), (STACK, 0), (PURGE, 0), (STACK, 0)];
    play(&mut k, &script, COMBAT_START + SimDuration(10_000));
    assert_eq!(rage_of(&k), 1.0 + 100.0 + 10.0);
}

#[test]
fn interrupting_a_cast_reaches_the_interrupter() {
    let mut f = fixture(vec![listener(ListenFor::Interrupted, vec![rage(1.0)])]);
    f.boss(vec![EnemyRule {
        name: EventName("cast".into()),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::Cast {
            time: SimDuration(5000),
            interruptible: true,
            then: Box::new(EnemyAction::Effects {
                effects: Vec::new(),
                target: EnemyTarget::Tank,
            }),
        },
    }]);
    let kick = Effect::Interrupt {
        target: EffectTarget::Target,
    };
    add_spell(&mut f, spell(KICK, vec![kick]));
    let mut k = f.kernel();
    // The second kick finds nothing to stop.
    play(
        &mut k,
        &[(KICK, 0), (KICK, 0)],
        COMBAT_START + SimDuration(4000),
    );
    assert_eq!(rage_of(&k), 1.0);
}

/// Submit each choice in turn from the start of combat, then wait.
fn steer(k: &mut Kernel<PartyMechanics>, script: &[Choice], until: SimTime) {
    let mut next = 0;
    loop {
        match k.advance().unwrap() {
            Step::Done(_) => return,
            Step::Decide(req) => {
                if req.now >= until {
                    return;
                }
                let choice = match script.get(next) {
                    Some(c) if req.now >= COMBAT_START => {
                        next += 1;
                        c.clone()
                    }
                    Some(_) => Choice::Wait(Wait::NextEvent),
                    None => Choice::Wait(Wait::Until(until)),
                };
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
}

#[test]
fn moving_and_stopping_reach_listeners_once_each() {
    let f = fixture(vec![
        listener(ListenFor::MoveStart, vec![rage(1.0)]),
        listener(ListenFor::MoveEnd, vec![rage(10.0)]),
    ]);
    let mut k = f.kernel();
    let script = [
        Choice::Move(MoveGoal::Yards(14.0)),
        Choice::Wait(Wait::Until(COMBAT_START + SimDuration(3000))),
        Choice::Move(MoveGoal::Yards(7.0)),
        Choice::StopMove,
    ];
    steer(&mut k, &script, COMBAT_START + SimDuration(4000));
    // Two seconds' walk to its end, then a walk stopped at once.
    assert_eq!(rage_of(&k), 22.0);
}

#[test]
fn health_lines_fire_once_as_a_hit_crosses_them() {
    let mut f = fixture(vec![
        listener(ListenFor::HealthBelow { pct: 70 }, vec![rage(1.0)]),
        listener(ListenFor::HealthBelow { pct: 50 }, vec![rage(10.0)]),
    ]);
    let max = f.seat_max_health() as f64;
    f.boss(vec![hits_seat(
        max * 0.22,
        SimDuration(1000),
        SimDuration(1000),
    )]);
    let mut k = f.kernel();
    play(&mut k, &[], COMBAT_START + SimDuration(3500));
    let me = k.state().actor(ActorId(0)).unwrap();
    // About 78%, 56%, then 34%.
    assert!(
        (0.3..0.5).contains(&me.health_frac()),
        "{}",
        me.health_frac()
    );
    assert_eq!(rage_of(&k), 11.0);
}

#[test]
fn a_death_save_holds_once_then_its_lockout_lets_the_next_hit_kill() {
    let mut f = fixture(Vec::new());
    let mut save = aura(SAVE);
    save.prevents_death = Some(PreventDeath {
        heal_to_pct: 0.3,
        lockout: Some(LOCKOUT),
        on_prevent: vec![rage(1.0)],
    });
    f.data.auras.insert(SAVE, save);
    f.template.passive_auras.push(SAVE);
    let mut lockout = aura(LOCKOUT);
    lockout.duration = Some(SimDuration(6000));
    f.data.auras.insert(LOCKOUT, lockout);
    let max = f.seat_max_health();
    f.boss(vec![hits_seat(
        max as f64 * 2.0,
        SimDuration(1000),
        SimDuration(2000),
    )]);
    let mut k = f.kernel();
    play(&mut k, &[], COMBAT_START + SimDuration(2000));
    let me = k.state().actor(ActorId(0)).unwrap();
    assert!(me.alive);
    assert_eq!(me.health, (max as f64 * 0.3).round() as u64);
    assert!(k
        .state()
        .auras(ActorId(0))
        .iter()
        .any(|a| a.aura == LOCKOUT));
    assert_eq!(rage_of(&k), 1.0);
    play(&mut k, &[], COMBAT_START + SimDuration(4000));
    let trace = k.drain_trace();
    let saved: Vec<SimTime> = trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::DeathPrevented { aura: SAVE, .. }))
        .map(|r| r.time)
        .collect();
    assert_eq!(saved, vec![COMBAT_START + SimDuration(1000)]);
    assert_eq!(
        deaths(&trace),
        vec![(COMBAT_START + SimDuration(3000), ActorId(0))]
    );
}

#[test]
fn chained_listeners_hear_resource_gains_and_extra_swings_without_self_looping() {
    use portunus_gamedata::item::{WeaponDef, WeaponHand};

    let mut on_energy = listener(
        ListenFor::ResourceGained(ResourceKind::Energy),
        vec![rage(10.0)],
    );
    on_energy.chain = true;
    let mut on_hit = listener(
        ListenFor::WeaponHit {
            hand: Some(WeaponHand::MainHand),
        },
        vec![rage(1.0)],
    );
    on_hit.chain = true;

    let mut f = fixture(vec![
        // Grants Energy on CastStart (depth 1), which `on_energy` (`chain: true`) hears.
        listener(
            ListenFor::CastStart {
                spell: Some(HARD),
                school: None,
            },
            vec![Effect::Resource(ResourceAmount {
                kind: ResourceKind::Energy,
                amount: 50.0,
            })],
        ),
        on_energy,
        on_hit,
    ]);
    // Put ExtraSwing on a separate aura so `on_hit` on `KIT` hears the extra
    // swing, while ExtraSwing itself never loops.
    let mut windfury = aura(STACKY);
    let mut wf_listener = listener(
        ListenFor::Swing {
            hand: Some(WeaponHand::MainHand),
            crit_only: false,
        },
        vec![Effect::ExtraSwing],
    );
    wf_listener.chain = true;
    windfury.listeners = vec![wf_listener];
    f.data.auras.insert(STACKY, windfury);
    f.template.passive_auras.push(STACKY);

    f.template.main_hand = Some(WeaponDef {
        speed: SimDuration(10_000),
        min_damage: 100.0,
        max_damage: 100.0,
        ranged: false,
    });
    f.template.resources.push(ResourceDef {
        kind: ResourceKind::Energy,
        max: 100.0,
        initial: 0.0,
        regen_per_sec: 0.0,
        regen_hasted: false,
        recharge: None,
        out_of_combat: None,
    });
    let mut hard = spell(HARD, Vec::new());
    hard.cast = CastKind::Cast {
        time: SimDuration(1500),
        hasted: false,
    };
    add_spell(&mut f, hard);

    let mut k = f.kernel();
    play(&mut k, &[(HARD, 0)], COMBAT_START + SimDuration(3000));
    // 10 rage from `on_energy` hearing the listener's Energy gain + 2 rage
    // from `on_hit` hearing both the initial swing and the one ExtraSwing.
    assert_eq!(rage_of(&k), 12.0);
}

#[test]
fn damage_dealt_filters_direct_versus_periodic_and_by_aura() {
    use portunus_gamedata::aura::Periodic;

    const DOT_AURA: AuraId = AuraId(900_850);
    const INFECT: SpellId = SpellId(900_850);

    let mut f = fixture(vec![
        listener(
            ListenFor::DamageDealt {
                spell: Some(INFECT),
                school: None,
                crit_only: false,
                killing_blow: false,
                direct: Some(true),
                aura: None,
            },
            vec![rage(100.0)],
        ),
        listener(
            ListenFor::DamageDealt {
                spell: None,
                school: None,
                crit_only: false,
                killing_blow: false,
                direct: Some(false),
                aura: Some(DOT_AURA),
            },
            vec![rage(1.0)],
        ),
    ]);
    let mut dot = aura(DOT_AURA);
    dot.duration = Some(SimDuration(3000));
    dot.periodic = Some(Periodic {
        period: SimDuration(1000),
        hasted: false,
        partial_final_tick: false,
        tick_zero: false,
        effects: vec![damage(50.0, EffectTarget::Target)],
    });
    f.data.auras.insert(DOT_AURA, dot);
    add_spell(
        &mut f,
        spell(
            INFECT,
            vec![
                damage(200.0, EffectTarget::Target),
                apply(DOT_AURA, EffectTarget::Target),
            ],
        ),
    );
    let mut k = f.kernel();
    play(&mut k, &[(INFECT, 0)], COMBAT_START + SimDuration(4000));
    // 100 from the single direct hit + 3 from the three periodic ticks.
    assert_eq!(rage_of(&k), 103.0);
}

#[test]
fn tick_zero_tick_period_modifiers_extra_ticks_and_stack_consumption_work_together() {
    use portunus_gamedata::aura::Periodic;
    use portunus_gamedata::effect::{ModKind, ModScope, Modifier, Predicate};

    const DOT_AURA: AuraId = AuraId(900_860);
    const EXSANGUINATE: AuraId = AuraId(900_861);
    const STEALTH_SNAPSHOT: AuraId = AuraId(900_862);
    const APPLY_DOT: SpellId = SpellId(900_860);
    const SPEED_UP: SpellId = SpellId(900_863);
    const PULSE_NOW: SpellId = SpellId(900_864);
    const BURST_STACKS: SpellId = SpellId(900_865);

    let mut f = fixture(vec![listener(
        ListenFor::PeriodicTick(DOT_AURA),
        vec![rage(1.0)],
    )]);

    let mut dot = aura(DOT_AURA);
    dot.duration = Some(SimDuration(4000));
    dot.max_stacks = 5;
    dot.periodic = Some(Periodic {
        period: SimDuration(2000),
        hasted: false,
        partial_final_tick: false,
        tick_zero: true,
        effects: vec![damage(100.0, EffectTarget::Target)],
    });
    f.data.auras.insert(DOT_AURA, dot);

    let mut fast = aura(EXSANGUINATE);
    fast.duration = Some(SimDuration(10_000));
    fast.modifiers = vec![Modifier {
        scope: ModScope::Spell(APPLY_DOT),
        kind: ModKind::TickPeriodPct,
        value: -50.0,
        per_stack: false,
        condition: None,
    }];
    f.data.auras.insert(EXSANGUINATE, fast);

    // Conditional PersistentPct: only applies when CasterHasAura(EXSANGUINATE).
    let mut cond_snap = aura(STEALTH_SNAPSHOT);
    cond_snap.modifiers = vec![Modifier {
        scope: ModScope::Spell(APPLY_DOT),
        kind: ModKind::PersistentPct,
        value: 100.0,
        per_stack: false,
        condition: Some(Predicate::CasterHasAura(EXSANGUINATE)),
    }];
    f.data.auras.insert(STEALTH_SNAPSHOT, cond_snap);
    f.template.passive_auras.push(STEALTH_SNAPSHOT);

    let mut apply_three = apply(DOT_AURA, EffectTarget::Target);
    if let Effect::ApplyAura { stacks, .. } = &mut apply_three {
        *stacks = 3;
    }
    add_spell(&mut f, spell(APPLY_DOT, vec![apply_three]));
    add_spell(
        &mut f,
        spell(SPEED_UP, vec![apply(EXSANGUINATE, EffectTarget::Caster)]),
    );
    add_spell(
        &mut f,
        spell(
            PULSE_NOW,
            vec![Effect::TickAura {
                aura: DOT_AURA,
                target: EffectTarget::Target,
                fraction: 0.5,
            }],
        ),
    );
    add_spell(
        &mut f,
        spell(
            BURST_STACKS,
            vec![Effect::ConsumeStacks {
                aura: DOT_AURA,
                target: EffectTarget::Target,
                max: Some(2),
                then: vec![damage(1000.0, EffectTarget::Target)],
            }],
        ),
    );

    let mut k = f.kernel();
    // At 10s: APPLY_DOT (without EXSANGUINATE, so pmultiplier = 1.0; tick_zero fires at 10s).
    // At 11s: SPEED_UP (-50% tick period -> rescales remaining tick from 12s to 11.5s, then every 1s).
    // At 12s: PULSE_NOW (extra 0.5 tick).
    // At 13s: BURST_STACKS (consumes 2 of the 3 stacks -> 2000 damage, leaves 1 stack).
    play(
        &mut k,
        &[
            (APPLY_DOT, 0),
            (SPEED_UP, 0),
            (PULSE_NOW, 0),
            (BURST_STACKS, 0),
        ],
        COMBAT_START + SimDuration(4500),
    );
    let trace = k.drain_trace();
    let boss = k.state().enemies()[0];
    let burst_hits = hits(&trace, BURST_STACKS, boss);
    assert_eq!(burst_hits.len(), 1);
    let dot_hits = hits(&trace, APPLY_DOT, boss);
    // tick_zero (10s), rescaled tick at 11.5s, PULSE_NOW at 12s (0.5x),
    // and rescaled ticks at 12.5s and 13.5s = 5 ticks!
    assert_eq!(dot_hits.len(), 5, "{dot_hits:?}");
    assert_eq!(rage_of(&k), 5.0);
    // PULSE_NOW dealt half a normal tick; BURST_STACKS dealt 20x a normal tick (2 stacks * 1000 vs 100).
    assert!(close(dot_hits[2], dot_hits[0] as f64 * 0.5));
    assert!(close(burst_hits[0], dot_hits[0] as f64 * 20.0));
}

#[test]
fn recent_damage_windows_and_stationary_predicates_track_recent_hits_and_movement() {
    use portunus_gamedata::effect::Predicate;

    const DEATH_STRIKE: SpellId = SpellId(900_870);
    const STILL_STRIKE: SpellId = SpellId(900_871);

    let mut f = fixture(Vec::new());
    // Boss hits for 1000 at +1s, +4s, +6s (relative to combat start at 10s: 11s, 14s, 16s).
    f.boss(vec![
        hits_seat(1000.0, SimDuration(1000), SimDuration(100_000)),
        hits_seat(1000.0, SimDuration(4000), SimDuration(100_000)),
        hits_seat(1000.0, SimDuration(6000), SimDuration(100_000)),
    ]);
    let mut ds = spell(
        DEATH_STRIKE,
        vec![Effect::GainResource {
            kind: ResourceKind::Rage,
            amount: Coefficient::RecentDamageTaken {
                window: SimDuration(5000),
                share: 0.5,
            },
        }],
    );
    ds.gcd = None;
    add_spell(&mut f, ds);
    let mut still = spell(
        STILL_STRIKE,
        vec![Effect::If {
            when: Predicate::Stationary {
                for_at_least: SimDuration(3000),
            },
            then: vec![rage(10.0)],
            otherwise: Vec::new(),
        }],
    );
    still.gcd = None;
    add_spell(&mut f, still);

    let mut k = f.kernel();
    let script = [
        // At 10s, seat has stood still since 0s (>= 3s) -> +10 rage.
        Choice::cast(STILL_STRIKE),
        Choice::Move(MoveGoal::Yards(7.0)),
        Choice::StopMove,
        // Just moved at 10s (< 3s stationary) -> +0 rage.
        Choice::cast(STILL_STRIKE),
        Choice::Wait(Wait::Until(COMBAT_START + SimDuration(7000))),
        // At 17s: hits at 11s (-6s), 14s (-3s), 16s (-1s); 5s window sums last two.
        Choice::cast(DEATH_STRIKE),
    ];
    steer(&mut k, &script, COMBAT_START + SimDuration(8000));
    // At 18s, only the 16s hit is within 2.5s.
    let single_hit_taken = k.state().recent_damage_taken(ActorId(0), SimDuration(2500));
    assert!(single_hit_taken > 0);
    // 10 rage from STILL_STRIKE + 0.5 * (2 * single_hit_taken) = 10 + single_hit_taken.
    assert_eq!(rage_of(&k), 10.0 + single_hit_taken as f64);
}
