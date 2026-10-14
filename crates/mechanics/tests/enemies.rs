//! The enemy side: phases where a boss can't be targeted, crowd control,
//! and dispels, on made-up spells grafted onto the checked-in Elemental
//! data.

mod synthetic;

use portunus_core::{ActorId, AuraId, Dist, EventName, SimDuration, SimTime, SpellId, Trigger};
use portunus_engine::trace::{CastEndReason, TraceEvent};
use portunus_engine::{
    CastOpts, Choice, Engine, Kernel, Readiness, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::aura::{DispelType, Periodic};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemySubject, EnemyTarget, RuleIndex};
use portunus_gamedata::item::WeaponDef;
use portunus_gamedata::stats::SchoolMask;
use portunus_mechanics::PartyMechanics;
use synthetic::*;

const VANISH: AuraId = AuraId(900_901);
const DOT: AuraId = AuraId(900_902);
const DOTTER: SpellId = SpellId(900_903);

fn secs(s: u32) -> SimTime {
    COMBAT_START + SimDuration(s * 1000)
}

/// When `spell` of `school` landed on `target`.
fn landed(
    trace: &[TraceRecord],
    spell: Option<SpellId>,
    school: SchoolMask,
    target: ActorId,
) -> Vec<SimTime> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d)
                if d.spell == spell && d.school == school && d.target == target =>
            {
                Some(r.time)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn an_untargetable_boss_sends_seats_to_its_adds_and_back_but_keeps_its_dots() {
    let mut f = fixture(Vec::new());
    f.adds(vec![("imp", 3000, Vec::new())]);
    let mut vanish = aura(VANISH);
    vanish.duration = Some(SimDuration(10_000));
    vanish.untargetable = true;
    f.data.auras.insert(VANISH, vanish);
    let mut dot = aura(DOT);
    dot.duration = Some(SimDuration(60_000));
    dot.periodic = Some(Periodic {
        period: SimDuration(1000),
        hasted: false,
        partial_final_tick: false,
        tick_zero: false,
        effects: vec![Effect::Damage {
            amount: Coefficient::Flat(100.0),
            school: SchoolMask::FIRE,
            target: EffectTarget::Target,
            aoe: None,
            ignores_armor: false,
            hand: None,
            per_count: None,
            unmodified: false,
        }],
    });
    f.data.auras.insert(DOT, dot);
    add_spell(
        &mut f,
        spell(DOTTER, vec![apply(DOT, EffectTarget::Target)]),
    );
    let rules = &mut f.enemies.enemies.get_mut(&dummy()).unwrap().rules;
    rules.push(EnemyRule {
        name: EventName("vanish".into()),
        phase: None,
        when: Trigger::Elapsed(SimDuration(5000)),
        repeat: None,
        action: EnemyAction::SelfAura(VANISH),
    });
    f.template.main_hand = Some(WeaponDef {
        speed: SimDuration(2000),
        min_damage: 1.0,
        max_damage: 1.0,
        ranged: false,
    });
    let mut k = f.kernel();
    let mut dotted = false;
    let mut refused = false;
    while let Step::Decide(req) = k.advance().unwrap() {
        if req.now >= secs(20) {
            break;
        }
        let state = k.state();
        let boss = state.enemies()[0];
        let me = state.seats()[0];
        if !state.actor(boss).unwrap().targetable && !refused {
            let at_boss = Choice::Cast {
                ability: STRIKE,
                target: TargetSel::Actor(boss),
                opts: CastOpts::default(),
            };
            assert!(k.submit(req.seat, at_boss).is_err());
            refused = true;
        }
        let state = k.state();
        let ready = |s| k.legal(req.seat).abilities.get(&s) == Some(&Readiness::Now);
        let choice = if req.now < COMBAT_START {
            Choice::Wait(Wait::Until(COMBAT_START))
        } else if !dotted && ready(DOTTER) {
            dotted = true;
            Choice::Cast {
                ability: DOTTER,
                target: TargetSel::Actor(boss),
                opts: CastOpts::default(),
            }
        } else if state.target(me).is_some() && ready(STRIKE) {
            Choice::Cast {
                ability: STRIKE,
                target: TargetSel::Primary,
                opts: CastOpts::default(),
            }
        } else {
            Choice::Wait(Wait::NextEvent)
        };
        k.submit(req.seat, choice).unwrap();
    }
    assert!(refused);
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    let (boss, imp) = (e[0], e[1]);
    let on_boss = landed(&trace, Some(STRIKE), SchoolMask::PHYSICAL, boss);
    let on_imp = landed(&trace, Some(STRIKE), SchoolMask::PHYSICAL, imp);
    // The boss takes strikes until it vanishes at 5 s; the seat turns to
    // the imp, kills it in three, waits with nothing to hit, and turns
    // back the moment the boss returns at 15 s.
    assert!(on_boss.iter().all(|&t| t < secs(5) || t >= secs(15)));
    assert_eq!(on_boss.iter().filter(|&&t| t < secs(5)).count(), 4);
    assert_eq!(on_imp, vec![secs(5), secs(6), secs(7)]);
    assert_eq!(on_boss.iter().find(|&&t| t >= secs(15)), Some(&secs(15)));
    // Swings follow the seat's target, and the attempts are what count:
    // a miss or a dodge lands nothing.
    let swings = landed(&trace, None, SchoolMask::PHYSICAL, boss);
    assert!(swings.iter().any(|&t| t < secs(5)), "{swings:?}");
    assert!(swings.iter().all(|&t| t < secs(5) || t >= secs(15)));
    assert!(swings.iter().any(|&t| t >= secs(15)), "{swings:?}");
    // Its DoT ticks on through the phase.
    let ticks = landed(&trace, None, SchoolMask::FIRE, boss);
    let hidden = ticks.iter().filter(|&&t| t > secs(5) && t < secs(15));
    assert_eq!(hidden.count(), 9, "{ticks:?}");
}

const NOVA: SpellId = SpellId(900_904);

#[test]
fn areas_pass_over_an_untargetable_enemy() {
    let mut f = fixture(Vec::new());
    f.adds(vec![("imp", 1_000_000, Vec::new())]);
    let mut vanish = aura(VANISH);
    vanish.untargetable = true;
    f.data.auras.insert(VANISH, vanish);
    f.enemies
        .enemies
        .get_mut(&dummy())
        .unwrap()
        .rules
        .push(EnemyRule {
            name: EventName("vanish".into()),
            phase: None,
            when: Trigger::Now,
            repeat: None,
            action: EnemyAction::SelfAura(VANISH),
        });
    add_spell(
        &mut f,
        spell(NOVA, vec![damage(100.0, EffectTarget::AllEnemies)]),
    );
    let mut k = f.kernel();
    play(&mut k, &[(NOVA, 1)], secs(3));
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    assert!(landed(&trace, Some(NOVA), SchoolMask::PHYSICAL, e[0]).is_empty());
    assert_eq!(
        landed(&trace, Some(NOVA), SchoolMask::PHYSICAL, e[1]).len(),
        1
    );
}

const BASH: SpellId = SpellId(900_911);
const SLAM_MARK: AuraId = AuraId(900_912);
const LATE_MARK: AuraId = AuraId(900_913);

/// Cast each `(when, spell, enemy index)` at that moment, or as soon after
/// as it's ready, then wait until `until`.
fn play_at(k: &mut Kernel<PartyMechanics>, script: &[(SimTime, SpellId, usize)], until: SimTime) {
    let mut next = 0;
    loop {
        let Step::Decide(req) = k.advance().unwrap() else {
            return;
        };
        if req.now >= until {
            return;
        }
        let ready = |s| k.legal(req.seat).abilities.get(&s) == Some(&Readiness::Now);
        let choice = match script.get(next) {
            Some(&(at, spell, i)) if req.now >= at && ready(spell) => {
                next += 1;
                Choice::Cast {
                    ability: spell,
                    target: TargetSel::Actor(k.state().enemies()[i]),
                    opts: CastOpts::default(),
                }
            }
            Some(&(at, ..)) if req.now < at => Choice::Wait(Wait::Until(at)),
            Some(_) => Choice::Wait(Wait::NextEvent),
            None => Choice::Wait(Wait::Until(until)),
        };
        k.submit(req.seat, choice).unwrap();
    }
}

/// A seat with `BASH`, a 4 s stun.
fn basher() -> Fixture {
    let mut f = fixture(Vec::new());
    let bash = Effect::Stun {
        duration: SimDuration(4000),
        target: EffectTarget::Target,
    };
    add_spell(&mut f, spell(BASH, vec![bash]));
    f
}

/// When each stun on `actor` was to end.
fn stuns(trace: &[TraceRecord], actor: ActorId) -> Vec<SimTime> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Stunned { actor: a, until } if a == actor => Some(until),
            _ => None,
        })
        .collect()
}

/// When `rule` fired, and when its casts ended and why.
fn rule_log(trace: &[TraceRecord], rule: u16) -> (Vec<SimTime>, Vec<(SimTime, CastEndReason)>) {
    let mut fired = Vec::new();
    let mut ended = Vec::new();
    for r in trace {
        match r.event {
            TraceEvent::EnemyRule { rule: i, .. } if i == RuleIndex(rule) => fired.push(r.time),
            TraceEvent::EnemyCastEnd {
                rule: i, reason, ..
            } if i == RuleIndex(rule) => ended.push((r.time, reason)),
            _ => {}
        }
    }
    (fired, ended)
}

/// A 3 s uninterruptible slam 2 s in and every 10 s after, a one-off at
/// 5 s, and an echo 4 s after the first slam.
fn slammer(f: &mut Fixture) {
    f.data.auras.insert(SLAM_MARK, aura(SLAM_MARK));
    f.data.auras.insert(LATE_MARK, aura(LATE_MARK));
    f.boss(vec![
        EnemyRule {
            name: EventName("slam".into()),
            phase: None,
            when: Trigger::Elapsed(SimDuration(2000)),
            repeat: Some(Dist::Fixed(SimDuration(10_000))),
            action: EnemyAction::Cast {
                time: SimDuration(3000),
                interruptible: false,
                then: Box::new(EnemyAction::SelfAura(SLAM_MARK)),
            },
        },
        EnemyRule {
            name: EventName("late".into()),
            phase: None,
            when: Trigger::Elapsed(SimDuration(5000)),
            repeat: None,
            action: EnemyAction::SelfAura(LATE_MARK),
        },
        EnemyRule {
            name: EventName("echo".into()),
            phase: None,
            when: Trigger::Delayed {
                delay: SimDuration(4000),
                after: Box::new(Trigger::Fired {
                    who: EnemySubject::Itself,
                    event: EventName("slam".into()),
                    nth: 1,
                }),
            },
            repeat: None,
            action: EnemyAction::SelfAura(LATE_MARK),
        },
    ]);
}

#[test]
fn a_stun_stops_even_an_uninterruptible_cast_and_holds_the_enemys_clocks() {
    let mut f = basher();
    slammer(&mut f);
    let mut k = f.kernel();
    play_at(&mut k, &[(secs(3), BASH, 0)], secs(20));
    let trace = k.drain_trace();
    let boss = k.state().enemies()[0];
    assert_eq!(stuns(&trace, boss), vec![secs(7)]);
    let (slams, ended) = rule_log(&trace, 0);
    // The repeat due at 12 s and the one-off due at 5 s each wait out
    // the 4 s the boss stood stunned; the echo, due mid-stun at 6 s,
    // waits for it to end.
    assert_eq!(slams, vec![secs(2), secs(16)]);
    assert_eq!(
        ended,
        vec![
            (secs(3), CastEndReason::Interrupted),
            (secs(19), CastEndReason::Completed)
        ]
    );
    assert_eq!(rule_log(&trace, 1).0, vec![secs(9)]);
    assert_eq!(rule_log(&trace, 2).0, vec![secs(7)]);
}

#[test]
fn bosses_immune_to_crowd_control_shrug_off_stuns() {
    let mut f = basher();
    slammer(&mut f);
    f.enemies.enemies.get_mut(&dummy()).unwrap().cc_immune = true;
    let mut k = f.kernel();
    play_at(&mut k, &[(secs(3), BASH, 0)], secs(20));
    let trace = k.drain_trace();
    let boss = k.state().enemies()[0];
    assert!(stuns(&trace, boss).is_empty());
    let (slams, ended) = rule_log(&trace, 0);
    assert_eq!(slams, vec![secs(2), secs(12)]);
    assert_eq!(ended[0], (secs(5), CastEndReason::Completed));
    assert_eq!(rule_log(&trace, 1).0, vec![secs(5)]);
    assert_eq!(rule_log(&trace, 2).0, vec![secs(6)]);
}

#[test]
fn repeated_stuns_halve_down_to_a_sixteenth_until_twenty_seconds_pass() {
    let mut f = basher();
    f.boss(Vec::new());
    let mut k = f.kernel();
    let at = [0, 5, 8, 10, 12, 14, 34, 55];
    let script: Vec<_> = at.iter().map(|&s| (secs(s), BASH, 0)).collect();
    play_at(&mut k, &script, secs(60));
    let trace = k.drain_trace();
    let boss = k.state().enemies()[0];
    let ms = |s: u32, extra: u32| secs(s) + SimDuration(extra);
    // 34 s is within 20 s of the last stun ending (14.25 s), so it is
    // still at the floor; 55 s is past it, so it lasts the full 4 s.
    let want = vec![
        ms(4, 0),
        ms(7, 0),
        ms(9, 0),
        ms(10, 500),
        ms(12, 250),
        ms(14, 250),
        ms(34, 250),
        ms(59, 0),
    ];
    assert_eq!(stuns(&trace, boss), want);
}

const FURY: AuraId = AuraId(900_921);
const FRENZY: AuraId = AuraId(900_922);
const HEX: AuraId = AuraId(900_923);
const HEXER: SpellId = SpellId(900_924);
const BLIGHT: AuraId = AuraId(900_925);
const PURGE: SpellId = SpellId(900_926);
const SOOTHE: SpellId = SpellId(900_927);
const DECURSE: SpellId = SpellId(900_928);
const POISON_CLEANSE: SpellId = SpellId(900_929);

fn dispel(types: Vec<DispelType>, target: EffectTarget) -> Effect {
    Effect::Dispel {
        types,
        count: 5,
        target,
    }
}

fn typed(id: AuraId, dispel: DispelType) -> portunus_gamedata::aura::AuraDef {
    let mut a = aura(id);
    a.dispel = Some(dispel);
    a
}

/// The boss buffs itself with magic `FURY` and an enrage, `FRENZY`, and
/// curses the seat with `BLIGHT`; `HEXER` puts the seat's own magic `HEX`
/// on the boss and its own `BLIGHT` on itself.
fn dispels() -> Fixture {
    let mut f = fixture(Vec::new());
    f.data.auras.insert(FURY, typed(FURY, DispelType::Magic));
    f.data
        .auras
        .insert(FRENZY, typed(FRENZY, DispelType::Enrage));
    f.data.auras.insert(HEX, typed(HEX, DispelType::Magic));
    f.data
        .auras
        .insert(BLIGHT, typed(BLIGHT, DispelType::Curse));
    let now = |name: &str, action| EnemyRule {
        name: EventName(name.into()),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action,
    };
    f.boss(vec![
        now("fury", EnemyAction::SelfAura(FURY)),
        now("frenzy", EnemyAction::SelfAura(FRENZY)),
        now(
            "blight",
            EnemyAction::Effects {
                effects: vec![apply(BLIGHT, EffectTarget::Target)],
                target: EnemyTarget::AllPlayers,
            },
        ),
    ]);
    let hexer = spell(
        HEXER,
        vec![
            apply(HEX, EffectTarget::Target),
            apply(BLIGHT, EffectTarget::Caster),
        ],
    );
    add_spell(&mut f, hexer);
    let purge = spell(
        PURGE,
        vec![dispel(vec![DispelType::Magic], EffectTarget::Target)],
    );
    let soothe = spell(
        SOOTHE,
        vec![dispel(vec![DispelType::Enrage], EffectTarget::Target)],
    );
    let decurse = spell(
        DECURSE,
        vec![dispel(vec![DispelType::Curse], EffectTarget::Caster)],
    );
    let poison = spell(
        POISON_CLEANSE,
        vec![
            dispel(vec![DispelType::Poison], EffectTarget::Target),
            dispel(vec![DispelType::Poison], EffectTarget::Caster),
        ],
    );
    for s in [purge, soothe, decurse, poison] {
        add_spell(&mut f, s);
    }
    f
}

/// This file's auras on `holder`, and who applied them.
fn held(k: &Kernel<PartyMechanics>, holder: ActorId) -> Vec<(AuraId, ActorId)> {
    let mut v: Vec<_> = k
        .state()
        .auras(holder)
        .iter()
        .filter(|a| [FURY, FRENZY, HEX, BLIGHT].contains(&a.aura))
        .map(|a| (a.aura, a.source))
        .collect();
    v.sort();
    v
}

#[test]
fn dispels_take_only_what_enemies_applied_of_the_types_they_name() {
    let run = |script: &[(SimTime, SpellId, usize)]| {
        let mut k = dispels().kernel();
        play_at(&mut k, script, secs(10));
        let state = k.state();
        let (boss, me) = (state.enemies()[0], state.seats()[0]);
        (held(&k, boss), held(&k, me), boss, me)
    };
    let hex = (secs(0), HEXER, 0);
    let (boss_has, me_has, boss, me) = run(&[hex]);
    assert_eq!(boss_has, vec![(FURY, boss), (FRENZY, boss), (HEX, me)]);
    assert_eq!(me_has, vec![(BLIGHT, me), (BLIGHT, boss)]);
    // A purge takes the boss's magic buff but not the seat's magic hex.
    let (boss_has, _, _, _) = run(&[hex, (secs(1), PURGE, 0)]);
    assert_eq!(boss_has, vec![(FRENZY, boss), (HEX, me)]);
    // A soothe takes the enrage.
    let (boss_has, _, _, _) = run(&[hex, (secs(1), SOOTHE, 0)]);
    assert_eq!(boss_has, vec![(FURY, boss), (HEX, me)]);
    // A decurse takes the boss's curse from the seat, not its own.
    let (_, me_has, _, _) = run(&[hex, (secs(1), DECURSE, 0)]);
    assert_eq!(me_has, vec![(BLIGHT, me)]);
    // Nothing is a poison.
    let (boss_has, me_has, _, _) = run(&[hex, (secs(1), POISON_CLEANSE, 0)]);
    assert_eq!(boss_has.len(), 3);
    assert_eq!(me_has.len(), 2);
}

#[test]
fn a_dispel_takes_at_most_its_count() {
    let mut f = dispels();
    let one = Effect::Dispel {
        types: vec![DispelType::Magic, DispelType::Enrage],
        count: 1,
        target: EffectTarget::Target,
    };
    add_spell(&mut f, spell(PURGE, vec![one]));
    let mut k = f.kernel();
    play_at(&mut k, &[(secs(0), PURGE, 0)], secs(5));
    let boss = k.state().enemies()[0];
    let left = held(&k, boss);
    assert_eq!(left.len(), 1, "{left:?}");
    assert!(left[0].0 == FURY || left[0].0 == FRENZY);
}
