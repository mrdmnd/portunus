//! Effects across several enemies, on made-up spells grafted onto the
//! checked-in Elemental data: a boss that spawns adds at the pull, and a
//! seat that never crits, with rage as a counter for what its listeners
//! saw.

mod synthetic;

use portunus_core::{ActorId, AuraId, EventName, SimDuration, SimTime, SpellId, Trigger};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{Engine, StateView, TraceRecord};
use portunus_gamedata::aura::Periodic;
use portunus_gamedata::effect::{
    AoeRule, Coefficient, CountScale, Effect, EffectTarget, ListenFor, ModKind, ModScope, Modifier,
    TargetCount,
};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule};
use portunus_gamedata::stats::SchoolMask;
use synthetic::*;

const BURN: AuraId = AuraId(900_805);

#[test]
fn deaths_reach_listeners_by_killer_and_by_what_the_enemy_carried() {
    let mut f = fixture(vec![
        listener(
            ListenFor::DamageDealt {
                spell: Some(STRIKE),
                school: None,
                crit_only: false,
                killing_blow: true,
            },
            vec![rage(1.0)],
        ),
        listener(
            ListenFor::EnemyDied {
                killed_by_self: true,
                had_aura: None,
            },
            vec![rage(10.0)],
        ),
        listener(
            ListenFor::EnemyDied {
                killed_by_self: false,
                had_aura: Some(MARK),
            },
            vec![rage(100.0)],
        ),
        listener(
            ListenFor::EnemyDied {
                killed_by_self: false,
                had_aura: None,
            },
            vec![rage(1000.0)],
        ),
    ]);
    // The second add marks itself and burns itself to death in 5 s.
    let mut burn = aura(BURN);
    burn.duration = Some(SimDuration(10_000));
    burn.periodic = Some(Periodic {
        period: SimDuration(1000),
        hasted: false,
        partial_final_tick: false,
        effects: vec![damage(1000.0, EffectTarget::Target)],
    });
    f.data.auras.insert(BURN, burn);
    let burning = EnemyRule {
        name: EventName("burn".into()),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::Sequence(vec![
            EnemyAction::SelfAura(MARK),
            EnemyAction::SelfAura(BURN),
        ]),
    };
    f.adds(vec![
        ("imp", 3000, Vec::new()),
        ("burning_imp", 5000, vec![burning]),
        ("bystander", 1_000_000, Vec::new()),
    ]);
    let mut k = f.kernel();
    let script = [(MARKER, 1), (STRIKE, 1), (STRIKE, 1), (STRIKE, 1)];
    play(&mut k, &script, COMBAT_START + SimDuration(20_000));
    let trace = k.drain_trace();
    let state = k.state();
    let (imp, burning) = (state.enemies()[1], state.enemies()[2]);
    let died: Vec<ActorId> = deaths(&trace).iter().map(|d| d.1).collect();
    assert_eq!(died.len(), 2, "{died:?}");
    assert!(died.contains(&imp) && died.contains(&burning));
    // One killing blow (the third strike), one kill of the seat's own, one
    // death with the seat's mark (the other's mark was its own), two
    // deaths in all; the bystander lives.
    assert_eq!(rage_of(&k), 1.0 + 10.0 + 100.0 + 2000.0);
}

const RAPTURE: SpellId = SpellId(900_811);
const CRANE: SpellId = SpellId(900_812);
const CLEAVE: SpellId = SpellId(900_813);
const FAN: SpellId = SpellId(900_814);
const FAN_BOLT: SpellId = SpellId(900_815);

/// Three adds; the third marks itself as it spawns.
fn three_adds(f: &mut Fixture) {
    let self_mark = EnemyRule {
        name: EventName("mark".into()),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::SelfAura(MARK),
    };
    f.adds(vec![
        ("imp", 1_000_000, Vec::new()),
        ("imp2", 1_000_000, Vec::new()),
        ("imp3", 1_000_000, vec![self_mark]),
    ]);
}

#[test]
fn spells_can_hit_only_the_enemies_with_my_aura() {
    let mut f = fixture(Vec::new());
    three_adds(&mut f);
    let mine = EffectTarget::EnemiesWithAura {
        aura: MARK,
        from_self: true,
    };
    let anyone = EffectTarget::EnemiesWithAura {
        aura: MARK,
        from_self: false,
    };
    add_spell(
        &mut f,
        spell(RAPTURE, vec![damage(10_000.0, mine), damage(1.0, anyone)]),
    );
    let mut k = f.kernel();
    let script = [(MARKER, 1), (RAPTURE, 0)];
    play(&mut k, &script, COMBAT_START + SimDuration(5000));
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    let landed: Vec<usize> = (0..4).map(|i| hits(&trace, RAPTURE, e[i]).len()).collect();
    // The boss and the second add have no mark; the third's is its own.
    assert_eq!(landed, vec![0, 2, 0, 1]);
}

#[test]
fn damage_can_grow_with_marked_enemies_and_targets_hit() {
    let mut f = fixture(Vec::new());
    three_adds(&mut f);
    let mut crane = damage(10_000.0, EffectTarget::Target);
    if let Effect::Damage { per_count, .. } = &mut crane {
        *per_count = Some(CountScale {
            count: TargetCount::EnemiesWithAura {
                aura: MARK,
                from_self: true,
            },
            pct: 10.0,
        });
    }
    let mut cleave = damage(10_000.0, EffectTarget::AllEnemies);
    if let Effect::Damage { per_count, .. } = &mut cleave {
        *per_count = Some(CountScale {
            count: TargetCount::TargetsHit,
            pct: 5.0,
        });
    }
    add_spell(&mut f, spell(CRANE, vec![crane]));
    add_spell(
        &mut f,
        spell(CLEAVE, vec![cleave, damage(10_000.0, EffectTarget::Target)]),
    );
    let mut k = f.kernel();
    let script = [
        (CRANE, 0),
        (MARKER, 1),
        (CRANE, 0),
        (MARKER, 2),
        (MARKER, 0),
        (CRANE, 0),
        (CLEAVE, 0),
    ];
    play(&mut k, &script, COMBAT_START + SimDuration(10_000));
    let trace = k.drain_trace();
    let boss = k.state().enemies()[0];
    let crane = hits(&trace, CRANE, boss);
    assert_eq!(crane.len(), 3);
    let base = crane[0] as f64;
    // 0, then 1, then 3 enemies with the seat's mark (the third add's
    // own mark doesn't count).
    assert!(close(crane[1], base * 1.1), "{crane:?}");
    assert!(close(crane[2], base * 1.3), "{crane:?}");
    // Four enemies hit: 20% more than the plain hit beside it.
    let cleave = hits(&trace, CLEAVE, boss);
    assert_eq!(cleave.len(), 2);
    assert!(close(cleave[0], cleave[1] as f64 * 1.2), "{cleave:?}");
}

#[test]
fn for_each_runs_its_effects_once_per_target() {
    let mut f = fixture(Vec::new());
    three_adds(&mut f);
    let each = |target| Effect::ForEach {
        target,
        then: vec![damage(100.0, EffectTarget::Target), rage(1.0)],
    };
    let allies = EffectTarget::AlliesWithAura {
        aura: KIT,
        from_self: true,
    };
    add_spell(
        &mut f,
        spell(FAN, vec![each(EffectTarget::OtherEnemies), each(allies)]),
    );
    // The same as a projectile, whose damage is rolled as it launches.
    let mut bolt = spell(FAN_BOLT, vec![each(EffectTarget::OtherEnemies)]);
    bolt.speed = Some(40.0);
    add_spell(&mut f, bolt);
    let mut k = f.kernel();
    play(
        &mut k,
        &[(FAN, 2), (FAN_BOLT, 1)],
        COMBAT_START + SimDuration(5000),
    );
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    let landed =
        |spell| -> Vec<usize> { (0..4).map(|i| hits(&trace, spell, e[i]).len()).collect() };
    assert_eq!(landed(FAN), vec![1, 1, 0, 1], "everyone but the target");
    assert_eq!(landed(FAN_BOLT), vec![1, 0, 1, 1]);
    // Three enemies, then the seat itself (which holds its own kit), then
    // three more as the bolt lands.
    assert_eq!(hits(&trace, FAN, ActorId(0)).len(), 1);
    assert_eq!(rage_of(&k), 7.0);
}

/// Damage the seat dealt to `target` outside any spell: its listeners'.
fn copies(trace: &[TraceRecord], target: ActorId) -> Vec<u64> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.spell.is_none() && d.target == target => Some(d.amount),
            _ => None,
        })
        .collect()
}

#[test]
fn blade_flurry_copies_a_share_of_each_hit_to_a_few_others_but_never_a_copy() {
    // 35% of what landed to up to four other enemies, as dealt: neither
    // the doubled damage below nor armor applies again.
    let flurry = Effect::Damage {
        amount: Coefficient::EventAmount(0.35),
        school: SchoolMask::PHYSICAL,
        target: EffectTarget::OtherEnemies,
        aoe: Some(AoeRule {
            max_targets: Some(4),
            sqrt_cap: None,
            secondary: 1.0,
        }),
        ignores_armor: true,
        hand: None,
        per_count: None,
        unmodified: true,
    };
    let mut f = fixture(vec![listener(
        ListenFor::DamageDealt {
            spell: None,
            school: None,
            crit_only: false,
            killing_blow: false,
        },
        vec![flurry],
    )]);
    f.data
        .auras
        .get_mut(&KIT)
        .unwrap()
        .modifiers
        .push(Modifier {
            scope: ModScope::All,
            kind: ModKind::DamageDonePct,
            value: 100.0,
            per_stack: false,
            condition: None,
        });
    f.enemies.enemies.get_mut(&dummy()).unwrap().defense.armor = 10_000.0;
    f.adds(Vec::from(
        ["a1", "a2", "a3", "a4", "a5"].map(|key| (key, 1_000_000, Vec::new())),
    ));
    let mut k = f.kernel();
    play(&mut k, &[(STRIKE, 0)], COMBAT_START + SimDuration(5000));
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    let strike = hits(&trace, STRIKE, e[0]);
    assert_eq!(strike.len(), 1);
    assert!(
        (500..2000).contains(&strike[0]),
        "doubled, less armor: {strike:?}"
    );
    assert!(copies(&trace, e[0]).is_empty(), "no copy of a copy");
    let copied: Vec<Vec<u64>> = e[1..].iter().map(|&a| copies(&trace, a)).collect();
    assert_eq!(
        copied.iter().filter(|c| c.len() == 1).count(),
        4,
        "{copied:?}"
    );
    assert!(copied.iter().all(|c| c.len() <= 1), "{copied:?}");
    for c in copied.iter().flatten() {
        assert!(close(*c, strike[0] as f64 * 0.35), "{c} of {strike:?}");
    }
}

const PLAGUE: AuraId = AuraId(900_806);
const EMPOWER: AuraId = AuraId(900_807);
const INFECT: SpellId = SpellId(900_816);
const EMPOWERING: SpellId = SpellId(900_817);
const SPREAD: SpellId = SpellId(900_818);

#[test]
fn spreading_copies_stacks_snapshot_and_time_left_to_enemies_without_it() {
    let mut f = fixture(Vec::new());
    let mut plague = aura(PLAGUE);
    plague.duration = Some(SimDuration(20_000));
    plague.max_stacks = 3;
    plague.periodic = Some(Periodic {
        period: SimDuration(2000),
        hasted: false,
        partial_final_tick: false,
        effects: vec![damage(100.0, EffectTarget::Target)],
    });
    f.data.auras.insert(PLAGUE, plague);
    let mut empower = aura(EMPOWER);
    empower.duration = Some(SimDuration(1500));
    empower.modifiers = vec![Modifier {
        scope: ModScope::All,
        kind: ModKind::PersistentPct,
        value: 50.0,
        per_stack: false,
        condition: None,
    }];
    f.data.auras.insert(EMPOWER, empower);
    let mut infect = apply(PLAGUE, EffectTarget::Target);
    if let Effect::ApplyAura { stacks, .. } = &mut infect {
        *stacks = 2;
    }
    add_spell(&mut f, spell(INFECT, vec![infect]));
    add_spell(
        &mut f,
        spell(EMPOWERING, vec![apply(EMPOWER, EffectTarget::Caster)]),
    );
    let spread = Effect::SpreadAura {
        aura: PLAGUE,
        to: EffectTarget::AllEnemies,
        max: Some(2),
    };
    add_spell(&mut f, spell(SPREAD, vec![spread]));
    f.adds(Vec::from(
        ["a1", "a2", "a3", "a4"].map(|key| (key, 1_000_000, Vec::new())),
    ));
    let mut k = f.kernel();
    // Empowered at 10 s (to 11.5 s), the boss infected at 11 s and the
    // first add at 12 s, unempowered; the spread goes out at 13 s.
    let script = [(EMPOWERING, 0), (INFECT, 0), (INFECT, 1), (SPREAD, 0)];
    play(&mut k, &script, COMBAT_START + SimDuration(3500));
    let state = k.state();
    let plagues: Vec<Option<(u8, f64, Option<SimTime>)>> = state
        .enemies()
        .iter()
        .map(|&e| {
            state
                .auras(e)
                .iter()
                .find(|a| a.aura == PLAGUE && a.source == ActorId(0))
                .map(|a| (a.stacks, a.pmultiplier, a.expires))
        })
        .collect();
    let boss = Some((2, 1.5, Some(COMBAT_START + SimDuration(21_000))));
    let own = Some((2, 1.0, Some(COMBAT_START + SimDuration(22_000))));
    // The first add keeps its own, the next two without it get the
    // boss's, and the last is past the limit.
    assert_eq!(plagues, vec![boss, own, boss, boss, None]);
}

#[test]
fn a_unique_aura_follows_its_casters_latest_target() {
    let run = |unique: bool| {
        let mut f = fixture(Vec::new());
        f.data.auras.get_mut(&MARK).unwrap().unique_per_source = unique;
        three_adds(&mut f);
        let mut k = f.kernel();
        play(
            &mut k,
            &[(MARKER, 1), (MARKER, 2)],
            COMBAT_START + SimDuration(5000),
        );
        let state = k.state();
        state
            .enemies()
            .iter()
            .map(|&e| state.auras(e).iter().any(|a| a.aura == MARK))
            .collect::<Vec<bool>>()
    };
    // The third add's own mark has another source, so it stays.
    assert_eq!(run(true), vec![false, false, true, true]);
    assert_eq!(run(false), vec![false, true, true, true]);
}
