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
