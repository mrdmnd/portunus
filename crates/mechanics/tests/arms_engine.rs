//! The engine features Arms needs, on made-up spells grafted onto the
//! checked-in Elemental data (a seat with no attack power of its own that
//! never crits): weapon attack power, pseudo-random procs, mastery and
//! auto-attack scopes, damage taken from one source, spread and stacked
//! count scales, chained hits, crit-only swing listeners, cooldown cuts
//! per point spent, and channels that tick as they start, gain ticks, and
//! do something as they end.

mod synthetic;

use portunus_core::{ActorId, AuraId, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Readiness, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::effect::{
    Coefficient, CooldownChange, CountForm, CountScale, Effect, EffectTarget, ListenFor, ModKind,
    ModScope, Modifier, Predicate, ProcChance, TargetCount,
};
use portunus_gamedata::item::WeaponDef;
use portunus_gamedata::spell::{CastKind, CooldownDef};
use portunus_gamedata::stats::{Cost, ResourceKind, SpendScaling};
use synthetic::*;

const BUFF: AuraId = AuraId(900_901);
const WEAPON_STRIKE: SpellId = SpellId(900_902);
const FLAT_STRIKE: SpellId = SpellId(900_903);
const PROC: SpellId = SpellId(900_904);
const SPREAD: SpellId = SpellId(900_905);
const PER_STACK: SpellId = SpellId(900_906);
const STACKED: AuraId = AuraId(900_907);
const STACKER: SpellId = SpellId(900_908);
const COOL: SpellId = SpellId(900_909);
const SPEND: SpellId = SpellId(900_910);
const CHANNEL: SpellId = SpellId(900_911);
const CLEAVE: SpellId = SpellId(900_912);
const MASTERFUL: SpellId = SpellId(900_913);
const DOUBLY_MASTERFUL: SpellId = SpellId(900_914);
const UNSTACKED: SpellId = SpellId(900_915);
const ON_MARKED: SpellId = SpellId(900_916);

/// A 3.6 s, 1000-damage weapon: 277.8 dps, so 1666 weapon attack power.
fn great_axe() -> WeaponDef {
    WeaponDef {
        speed: SimDuration(3600),
        min_damage: 1000.0,
        max_damage: 1000.0,
        ranged: false,
    }
}

fn modifier(scope: ModScope, kind: ModKind, value: f64) -> Modifier {
    Modifier {
        scope,
        kind,
        value,
        per_stack: false,
        condition: None,
    }
}

/// A passive aura carrying `modifiers`.
fn passive(f: &mut Fixture, modifiers: Vec<Modifier>) {
    let mut buff = aura(BUFF);
    buff.modifiers = modifiers;
    f.data.auras.insert(BUFF, buff);
    f.template.passive_auras.push(BUFF);
}

fn amount(c: Coefficient) -> Effect {
    Effect::Damage {
        amount: c,
        school: portunus_gamedata::stats::SchoolMask::PHYSICAL,
        target: EffectTarget::Target,
        aoe: None,
        ignores_armor: false,
        hand: None,
        per_count: None,
        unmodified: false,
    }
}

/// The seat's auto-attacks, in order.
fn swings(trace: &[TraceRecord]) -> Vec<u64> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.spell.is_none() && d.source == ActorId(0) => Some(d.amount),
            _ => None,
        })
        .collect()
}

/// When `spell`'s hits landed.
fn hit_times(trace: &[TraceRecord], spell: SpellId) -> Vec<SimTime> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.spell == Some(spell) => Some(r.time),
            _ => None,
        })
        .collect()
}

fn cast_starts(trace: &[TraceRecord], spell: SpellId) -> Vec<SimTime> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::CastStart { spell: s, .. } if s == spell => Some(r.time),
            _ => None,
        })
        .collect()
}

fn armed() -> Fixture {
    let mut f = fixture(Vec::new());
    f.template.main_hand = Some(great_axe());
    add_spell(
        &mut f,
        spell(WEAPON_STRIKE, vec![amount(Coefficient::AttackPower(100.0))]),
    );
    add_spell(
        &mut f,
        spell(FLAT_STRIKE, vec![amount(Coefficient::Flat(166_600.0))]),
    );
    f
}

#[test]
fn attack_power_coefficients_count_the_weapon_floored_and_scaled() {
    let strikes = |f: &Fixture| {
        let mut k = f.kernel();
        let script = [(WEAPON_STRIKE, 0), (FLAT_STRIKE, 0)];
        play(&mut k, &script, COMBAT_START + SimDuration(5000));
        let trace = k.drain_trace();
        let boss = k.state().enemies()[0];
        (
            hits(&trace, WEAPON_STRIKE, boss),
            hits(&trace, FLAT_STRIKE, boss),
        )
    };
    // No attack power of its own: the weapon's floor(277.8 * 6) = 1666 is
    // all there is.
    let (weapon, flat) = strikes(&armed());
    assert_eq!((weapon.len(), flat.len()), (1, 1));
    assert!(close(weapon[0], flat[0] as f64), "{weapon:?} {flat:?}");
    let mut f = armed();
    passive(
        &mut f,
        vec![modifier(ModScope::All, ModKind::AttackPowerPct, 10.0)],
    );
    let (weapon, flat) = strikes(&f);
    assert!(
        close(weapon[0], flat[0] as f64 * 1.1),
        "{weapon:?} {flat:?}"
    );
    // Unarmed, nothing (and no damage event for nothing).
    let mut f = armed();
    f.template.main_hand = None;
    let (weapon, flat) = strikes(&f);
    assert_eq!((weapon.len(), flat.len()), (0, 1));
}

#[test]
fn attack_power_multipliers_scale_the_weapon_roll_of_auto_attacks() {
    let first_swing = |f: &Fixture| {
        let mut k = f.kernel();
        play(&mut k, &[(STRIKE, 0)], COMBAT_START + SimDuration(5000));
        swings(&k.drain_trace())[0]
    };
    let plain = first_swing(&armed());
    let mut f = armed();
    passive(
        &mut f,
        vec![modifier(ModScope::All, ModKind::AttackPowerPct, 10.0)],
    );
    assert!(close(first_swing(&f), plain as f64 * 1.1));
}

#[test]
fn auto_attacks_and_abilities_have_their_own_scopes() {
    let run = |f: &Fixture| {
        let mut k = f.kernel();
        play(&mut k, &[(STRIKE, 0)], COMBAT_START + SimDuration(5000));
        let trace = k.drain_trace();
        let boss = k.state().enemies()[0];
        (swings(&trace)[0], hits(&trace, STRIKE, boss)[0])
    };
    let (swing, strike) = run(&armed());
    let mut f = armed();
    passive(
        &mut f,
        vec![
            modifier(ModScope::AutoAttack, ModKind::DamageDonePct, 50.0),
            modifier(ModScope::Abilities, ModKind::DamageDonePct, 20.0),
        ],
    );
    let (boosted_swing, boosted_strike) = run(&f);
    assert!(close(boosted_swing, swing as f64 * 1.5), "{boosted_swing}");
    assert!(
        close(boosted_strike, strike as f64 * 1.2),
        "{boosted_strike}"
    );
}

#[test]
fn mastery_scales_damage_by_its_share_of_the_seats_mastery() {
    let mut f = fixture(Vec::new());
    add_spell(
        &mut f,
        spell(MASTERFUL, vec![damage(1_000_000.0, EffectTarget::Target)]),
    );
    add_spell(
        &mut f,
        spell(
            DOUBLY_MASTERFUL,
            vec![damage(1_000_000.0, EffectTarget::Target)],
        ),
    );
    passive(
        &mut f,
        vec![
            modifier(ModScope::Spell(MASTERFUL), ModKind::MasteryDamagePct, 1.0),
            modifier(
                ModScope::Spell(DOUBLY_MASTERFUL),
                ModKind::MasteryDamagePct,
                2.0,
            ),
        ],
    );
    let mut k = f.kernel();
    let script = [(STRIKE, 0), (MASTERFUL, 0), (DOUBLY_MASTERFUL, 0)];
    play(&mut k, &script, COMBAT_START + SimDuration(5000));
    let trace = k.drain_trace();
    let boss = k.state().enemies()[0];
    let plain = hits(&trace, STRIKE, boss)[0] as f64 * 1000.0;
    let once = hits(&trace, MASTERFUL, boss)[0] as f64 / plain;
    let twice = hits(&trace, DOUBLY_MASTERFUL, boss)[0] as f64 / plain;
    // Elemental's base mastery alone is 8%, and its rating adds less than
    // 40% more.
    assert!(once > 1.08 && once < 1.5, "{once}");
    assert!((twice - (2.0 * once - 1.0)).abs() < 2e-3, "{once} {twice}");
}

#[test]
fn pseudo_random_procs_hit_their_rate_without_long_droughts() {
    let mut proc = listener(
        ListenFor::DamageDealt {
            spell: Some(STRIKE),
            school: None,
            crit_only: false,
            killing_blow: false,
            direct: None,
            aura: None,
        },
        vec![Effect::TriggerSpell {
            spell: PROC,
            target: EffectTarget::Target,
            delay: SimDuration::ZERO,
        }],
    );
    proc.chance = ProcChance::Prd { chance: 0.25 };
    let mut f = fixture(vec![proc]);
    f.data
        .spells
        .insert(PROC, spell(PROC, vec![damage(1.0, EffectTarget::Target)]));
    let mut k = f.kernel();
    let script = vec![(STRIKE, 0); 400];
    play(&mut k, &script, COMBAT_START + SimDuration(500_000));
    let trace = k.drain_trace();
    let strikes = hit_times(&trace, STRIKE);
    let procs = hit_times(&trace, PROC);
    assert_eq!(strikes.len(), 400);
    let rate = procs.len() as f64 / strikes.len() as f64;
    assert!((rate - 0.25).abs() < 0.04, "{rate}");
    // The chance grows by C = 0.0847 per miss, so the 12th attempt after
    // a proc always procs.
    let mut gap = 0;
    let mut longest = 0;
    for t in &strikes {
        if procs.contains(t) {
            gap = 0;
        } else {
            gap += 1;
            longest = longest.max(gap);
        }
    }
    assert!(longest < 12, "{longest}");
}

#[test]
fn a_source_can_make_its_target_take_more_from_it_alone() {
    let mut f = fixture(Vec::new());
    f.seats = 2;
    let mut mark = aura(MARK);
    mark.modifiers = vec![modifier(ModScope::All, ModKind::SourceDamageTakenPct, 30.0)];
    f.data.auras.insert(MARK, mark);
    let mut k = f.kernel();
    let mut queues = [vec![STRIKE, MARKER, STRIKE], vec![STRIKE; 5]];
    let until = COMBAT_START + SimDuration(10_000);
    loop {
        match k.advance().unwrap() {
            Step::Done(_) => break,
            Step::Decide(req) if req.now >= until => break,
            Step::Decide(req) => {
                let queue = &mut queues[usize::from(req.seat.0)];
                let ready = |s| k.legal(req.seat).abilities.get(&s) == Some(&Readiness::Now);
                let choice = match queue.first() {
                    Some(&s) if req.now >= COMBAT_START && ready(s) => {
                        queue.remove(0);
                        Choice::Cast {
                            ability: s,
                            target: TargetSel::Primary,
                            opts: CastOpts::default(),
                        }
                    }
                    Some(_) => Choice::Wait(Wait::NextEvent),
                    None => Choice::Wait(Wait::Until(until)),
                };
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    let trace = k.drain_trace();
    let seats = k.state().seats().to_vec();
    let by = |seat: ActorId| -> Vec<u64> {
        trace
            .iter()
            .filter_map(|r| match r.event {
                TraceEvent::Damage(d) if d.spell == Some(STRIKE) && d.source == seat => {
                    Some(d.amount)
                }
                _ => None,
            })
            .collect()
    };
    let (mine, theirs) = (by(seats[0]), by(seats[1]));
    assert_eq!((mine.len(), theirs.len()), (2, 5));
    assert!(close(mine[1], mine[0] as f64 * 1.3), "{mine:?}");
    assert!(theirs.iter().all(|&t| t == mine[0]), "{theirs:?}");
}

/// Three adds beside the boss.
fn pack(f: &mut Fixture) {
    f.adds(vec![
        ("imp", 1_000_000, Vec::new()),
        ("imp2", 1_000_000, Vec::new()),
        ("imp3", 1_000_000, Vec::new()),
    ]);
}

fn scaled(target: EffectTarget, count: TargetCount, pct: f64, form: CountForm) -> Effect {
    let mut e = damage(10_000.0, target);
    if let Effect::Damage { per_count, .. } = &mut e {
        *per_count = Some(CountScale { count, pct, form });
    }
    e
}

#[test]
fn counts_can_spread_a_bonus_or_count_stacks_on_each_target() {
    let mut f = fixture(Vec::new());
    pack(&mut f);
    let mut stacked = aura(STACKED);
    stacked.max_stacks = 5;
    f.data.auras.insert(STACKED, stacked);
    add_spell(
        &mut f,
        spell(
            STACKER,
            vec![Effect::ApplyAura {
                aura: STACKED,
                target: EffectTarget::Target,
                stacks: 1,
                duration: None,
                per_unit_spent: None,
            }],
        ),
    );
    add_spell(
        &mut f,
        spell(
            SPREAD,
            vec![scaled(
                EffectTarget::Target,
                TargetCount::Enemies,
                20.0,
                CountForm::Inverse,
            )],
        ),
    );
    add_spell(
        &mut f,
        spell(
            PER_STACK,
            vec![scaled(
                EffectTarget::AllEnemies,
                TargetCount::TargetStacks { aura: STACKED },
                50.0,
                CountForm::Times,
            )],
        ),
    );
    add_spell(
        &mut f,
        spell(
            UNSTACKED,
            vec![scaled(
                EffectTarget::Target,
                TargetCount::TargetStacks { aura: STACKED },
                20.0,
                CountForm::Inverse,
            )],
        ),
    );
    add_spell(
        &mut f,
        spell(CLEAVE, vec![damage(10_000.0, EffectTarget::Target)]),
    );
    let mut k = f.kernel();
    let script = [
        (CLEAVE, 0),
        (SPREAD, 0),
        (UNSTACKED, 0),
        (STACKER, 1),
        (STACKER, 1),
        (STACKER, 2),
        (PER_STACK, 0),
    ];
    play(&mut k, &script, COMBAT_START + SimDuration(10_000));
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    let plain = hits(&trace, CLEAVE, e[0])[0] as f64;
    // 20% split across four enemies.
    assert!(close(hits(&trace, SPREAD, e[0])[0], plain * 1.05));
    // Split across no stacks: no bonus.
    assert!(close(hits(&trace, UNSTACKED, e[0])[0], plain));
    // Half the hit per stack on each target: none, two, one, none.
    let per: Vec<Vec<u64>> = e.iter().map(|&t| hits(&trace, PER_STACK, t)).collect();
    assert!(per[0].is_empty() && per[3].is_empty(), "{per:?}");
    assert!(close(per[1][0], plain), "{per:?}");
    assert!(close(per[2][0], plain * 0.5), "{per:?}");
}

#[test]
fn single_target_hits_chain_to_the_nearest_others_at_a_share() {
    let mut f = fixture(vec![listener(
        ListenFor::DamageDealt {
            spell: Some(ON_MARKED),
            school: None,
            crit_only: false,
            killing_blow: false,
            direct: None,
            aura: None,
        },
        vec![rage(1.0)],
    )]);
    pack(&mut f);
    let marked = EffectTarget::EnemiesWithAura {
        aura: MARK,
        from_self: true,
    };
    add_spell(&mut f, spell(ON_MARKED, vec![damage(10_000.0, marked)]));
    passive(
        &mut f,
        vec![
            modifier(ModScope::Spell(STRIKE), ModKind::ChainTargetsAdd, 1.0),
            modifier(ModScope::Spell(STRIKE), ModKind::ChainSharePct, 75.0),
            modifier(ModScope::Spell(ON_MARKED), ModKind::ChainTargetsAdd, 1.0),
        ],
    );
    let mut k = f.kernel();
    play(
        &mut k,
        &[(STRIKE, 2), (MARKER, 1), (ON_MARKED, 0)],
        COMBAT_START + SimDuration(5000),
    );
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    let strike: Vec<Vec<u64>> = e.iter().map(|&t| hits(&trace, STRIKE, t)).collect();
    // The target takes the hit, the first other enemy three quarters of it.
    assert_eq!(strike[2].len(), 1);
    assert!(
        close(strike[0][0], strike[2][0] as f64 * 0.75),
        "{strike:?}"
    );
    assert!(strike[1].is_empty() && strike[3].is_empty(), "{strike:?}");
    // A hit on a set of enemies doesn't chain: just the marked one.
    assert_eq!(rage_of(&k), 1.0);
}

#[test]
fn modifiers_and_listeners_can_ask_about_the_pack_and_the_current_target() {
    let crowded = |n: u8| Modifier {
        condition: Some(Predicate::EnemiesAtLeast(n)),
        ..modifier(ModScope::Spell(STRIKE), ModKind::DamageDonePct, 100.0)
    };
    let mut on_target = listener(
        ListenFor::DamageDealt {
            spell: Some(CLEAVE),
            school: None,
            crit_only: false,
            killing_blow: false,
            direct: None,
            aura: None,
        },
        vec![rage(1.0)],
    );
    on_target.condition = Some(Predicate::TargetIsCurrent);
    let mut f = fixture(vec![on_target]);
    pack(&mut f);
    add_spell(
        &mut f,
        spell(CLEAVE, vec![damage(10_000.0, EffectTarget::AllEnemies)]),
    );
    passive(&mut f, vec![crowded(4), crowded(5)]);
    let mut k = f.kernel();
    play(
        &mut k,
        &[(CLEAVE, 0), (STRIKE, 0)],
        COMBAT_START + SimDuration(5000),
    );
    let trace = k.drain_trace();
    let boss = k.state().enemies()[0];
    let cleave = hits(&trace, CLEAVE, boss)[0] as f64;
    // Four enemies: the first doubles the strike, the second doesn't.
    assert!(close(hits(&trace, STRIKE, boss)[0], cleave * 0.2));
    // Only the hit on the seat's target counts.
    assert_eq!(rage_of(&k), 1.0);
}

#[test]
fn swing_listeners_can_wait_for_crits() {
    let rage_from_swings = |crits: bool| {
        let mut f = fixture(vec![listener(
            ListenFor::Swing {
                hand: None,
                crit_only: true,
            },
            vec![rage(1.0)],
        )]);
        f.template.main_hand = Some(great_axe());
        if crits {
            passive(
                &mut f,
                vec![modifier(ModScope::All, ModKind::CritChanceAdd, 2000.0)],
            );
        }
        let mut k = f.kernel();
        play(&mut k, &[(STRIKE, 0)], COMBAT_START + SimDuration(20_000));
        let swung = swings(&k.drain_trace()).len();
        (rage_of(&k), swung)
    };
    assert_eq!(rage_from_swings(false).0, 0.0);
    let (rage, swung) = rage_from_swings(true);
    assert!(swung > 3);
    assert_eq!(rage, swung as f64);
}

#[test]
fn cooldown_cuts_scale_with_what_was_spent() {
    let mut f = fixture(vec![listener(
        ListenFor::ResourceSpent(ResourceKind::Rage),
        vec![Effect::AdjustCooldown {
            spell: COOL,
            change: CooldownChange::Reduce(SimDuration(1000)),
        }],
    )]);
    if let Some(r) = f
        .template
        .resources
        .iter_mut()
        .find(|r| r.kind == ResourceKind::Rage)
    {
        r.initial = 100.0;
    }
    let mut cool = spell(COOL, vec![damage(1.0, EffectTarget::Target)]);
    cool.cooldown = Some(CooldownDef {
        duration: SimDuration(60_000),
        charges: 1,
        hasted: false,
        category: None,
    });
    add_spell(&mut f, cool);
    let mut spend = spell(SPEND, vec![damage(1.0, EffectTarget::Target)]);
    spend.costs = vec![Cost {
        kind: ResourceKind::Rage,
        amount: 20.0,
        extra: 0.0,
        scaling: SpendScaling::None,
    }];
    add_spell(&mut f, spend);
    let mut k = f.kernel();
    let script = [(COOL, 0), (SPEND, 0), (COOL, 0)];
    play(&mut k, &script, COMBAT_START + SimDuration(90_000));
    let casts = cast_starts(&k.drain_trace(), COOL);
    assert_eq!(casts.len(), 2);
    // 20 rage spent: 20 s off the 60 s cooldown.
    assert_eq!(casts[1] - casts[0], SimDuration(40_000));
}

/// A 3 s, three-tick channel of 1000 damage a tick that ticks as it
/// starts and grants 100 rage as it ends.
fn channeler() -> Fixture {
    let mut f = fixture(Vec::new());
    let mut ch = spell(CHANNEL, vec![damage(1000.0, EffectTarget::Target)]);
    ch.cast = CastKind::Channel {
        duration: SimDuration(3000),
        ticks: 3,
        hasted: false,
        swings: false,
        tick_zero: true,
        on_end: vec![rage(100.0)],
    };
    add_spell(&mut f, ch);
    f
}

fn channel_run(f: &Fixture) -> (Vec<SimDuration>, f64) {
    let mut k = f.kernel();
    play(&mut k, &[(CHANNEL, 0)], COMBAT_START + SimDuration(10_000));
    let trace = k.drain_trace();
    let start = cast_starts(&trace, CHANNEL)[0];
    let at = hit_times(&trace, CHANNEL)
        .into_iter()
        .map(|t| t - start)
        .collect();
    (at, rage_of(&k))
}

#[test]
fn channels_can_tick_as_they_start_gain_ticks_and_act_as_they_end() {
    let secs = |s: &[u32]| -> Vec<SimDuration> { s.iter().map(|&x| SimDuration(x)).collect() };
    let (at, rage) = channel_run(&channeler());
    assert_eq!(at, secs(&[0, 1000, 2000, 3000]));
    assert_eq!(rage, 100.0);
    // Two more ticks in the same 3 s.
    let mut f = channeler();
    passive(
        &mut f,
        vec![modifier(
            ModScope::Spell(CHANNEL),
            ModKind::ChannelTicksAdd,
            2.0,
        )],
    );
    let (at, rage) = channel_run(&f);
    assert_eq!(at, secs(&[0, 600, 1200, 1800, 2400, 3000]));
    assert_eq!(rage, 100.0);
}
