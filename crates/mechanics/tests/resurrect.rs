//! Battle resurrections and Soulstone, on made-up spells grafted onto the
//! checked-in Elemental data.

mod synthetic;

use portunus_core::{
    ActorId, AuraId, Dist, EventName, Seat, SimDuration, SimTime, SpellId, Trigger,
};
use portunus_engine::trace::{TraceEvent, TraceRecord};
use portunus_engine::{BattleRes, Engine, Kernel, Readiness, StateView};
use portunus_gamedata::effect::{Effect, EffectTarget};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemyTarget};
use portunus_gamedata::item::WeaponDef;
use portunus_gamedata::spell::{CastKind, Targeting};
use portunus_gamedata::stats::SchoolMask;
use portunus_mechanics::PartyMechanics;
use synthetic::*;

const REBIRTH: SpellId = SpellId(900_971);
const SLOW_REBIRTH: SpellId = SpellId(900_972);
const STONE: AuraId = AuraId(900_973);
const STONE_UP: SpellId = SpellId(900_974);

fn t(ms: u32) -> SimTime {
    COMBAT_START + SimDuration(ms)
}

/// Two seconds into the pull, `target` takes ten times a seat's health.
fn lethal(f: &mut Fixture, target: EnemyTarget) {
    let max = f.seat_max_health();
    f.boss(vec![EnemyRule {
        name: EventName("wipe".into()),
        phase: None,
        when: Trigger::Elapsed(SimDuration(2000)),
        repeat: None,
        action: EnemyAction::Damage {
            amount: Dist::Fixed(10.0 * max as f64),
            school: SchoolMask::FIRE,
            target,
        },
    }]);
}

/// Every seat gets `REBIRTH`, an instant resurrection to 60% health, and
/// `SLOW_REBIRTH`, the same with a 1.5 s cast.
fn rebirths(f: &mut Fixture) {
    for (id, cast) in [
        (REBIRTH, CastKind::Instant),
        (
            SLOW_REBIRTH,
            CastKind::Cast {
                time: SimDuration(1500),
                hasted: false,
            },
        ),
    ] {
        let mut s = spell(
            id,
            vec![Effect::Resurrect {
                target: EffectTarget::Target,
                health_pct: 0.6,
            }],
        );
        s.cast = cast;
        s.targeting = Targeting::DeadAlly;
        s.hostile = false;
        add_spell(f, s);
    }
}

fn seat_index(k: &Kernel<PartyMechanics>, actor: ActorId) -> usize {
    k.state().seats().iter().position(|&a| a == actor).unwrap()
}

fn dead_and_living(k: &Kernel<PartyMechanics>, trace: &[TraceRecord]) -> (Vec<usize>, Vec<usize>) {
    let dead: Vec<usize> = deaths(trace)
        .into_iter()
        .map(|(_, a)| seat_index(k, a))
        .collect();
    let living = (0..k.state().seats().len())
        .filter(|i| !dead.contains(i))
        .collect();
    (dead, living)
}

fn alive(k: &Kernel<PartyMechanics>, seat: usize) -> bool {
    k.state().actor(k.state().seats()[seat]).unwrap().alive
}

fn readiness(k: &Kernel<PartyMechanics>, seat: usize, spell: SpellId) -> Option<Readiness> {
    k.legal(Seat(seat as u8)).abilities.get(&spell).copied()
}

fn resurrections(trace: &[TraceRecord]) -> Vec<(SimTime, ActorId)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Resurrect { actor } => Some((r.time, actor)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_battle_res_brings_the_dead_back_mid_pull_for_the_groups_one_charge() {
    let mut f = fixture(Vec::new());
    f.seats = 3;
    // No passive auras to come back with, whose gain would wake the seat
    // anyway.
    f.template.passive_auras.clear();
    f.template.main_hand = Some(WeaponDef {
        speed: SimDuration(2600),
        min_damage: 100.0,
        max_damage: 100.0,
        ranged: false,
    });
    rebirths(&mut f);
    lethal(&mut f, EnemyTarget::RandomPlayers(2));
    let max = f.seat_max_health();
    let mut k = f.kernel();

    play_party(&mut k, &[], t(1000));
    for seat in 0..3 {
        assert_eq!(readiness(&k, seat, REBIRTH), Some(Readiness::Blocked));
    }
    play_party(&mut k, &[], t(2500));
    let trace = k.drain_trace();
    let (dead, living) = dead_and_living(&k, &trace);
    assert_eq!(dead.len(), 2, "{dead:?}");
    let healer = living[0];
    assert_eq!(readiness(&k, healer, REBIRTH), Some(Readiness::Now));
    assert_eq!(k.state().battle_res().charges, 1);

    // The revived seat is asked at once, and fights on.
    play_party(
        &mut k,
        &[(healer, REBIRTH, Some(dead[0])), (dead[0], STRIKE, None)],
        t(4000),
    );
    let back = k.state().seats()[dead[0]];
    let trace = k.drain_trace();
    let revived = resurrections(&trace);
    assert_eq!(revived.len(), 1);
    let struck: Vec<SimTime> = trace
        .iter()
        .filter(
            |r| matches!(r.event, TraceEvent::Damage(d) if d.source == back && d.spell == Some(STRIKE)),
        )
        .map(|r| r.time)
        .collect();
    assert_eq!(struck, [revived[0].0]);
    let a = k.state().actor(back).unwrap();
    assert!(a.alive);
    assert_eq!(a.health, (max as f64 * 0.6).round() as u64);
    assert!(!alive(&k, dead[1]));
    // The charge is gone; the next comes 10 minutes into the run.
    let pool = k.state().battle_res();
    assert_eq!(pool.charges, 0);
    assert_eq!(pool.next_at, Some(SimTime(600_000)));
    assert!(
        matches!(readiness(&k, healer, REBIRTH), Some(Readiness::In(_))),
        "{:?}",
        readiness(&k, healer, REBIRTH)
    );

    play_party(&mut k, &[], t(8000));
    let swung = k
        .drain_trace()
        .iter()
        .any(|r| matches!(r.event, TraceEvent::Damage(d) if d.source == back && d.spell.is_none()));
    assert!(swung, "auto-attacks start again");
}

#[test]
fn an_offer_with_no_charge_left_when_accepted_lapses() {
    let mut f = fixture(Vec::new());
    f.seats = 4;
    // A full pool of one: spending it starts the recharge.
    f.externals.battle_res = BattleRes {
        initial: 1,
        every: SimDuration(30_000),
        max: 1,
    };
    rebirths(&mut f);
    lethal(&mut f, EnemyTarget::RandomPlayers(2));
    let mut k = f.kernel();
    play_party(&mut k, &[], t(2000));
    let trace = k.drain_trace();
    let (dead, living) = dead_and_living(&k, &trace);
    assert_eq!(dead.len(), 2, "{dead:?}");
    // Both start while the charge is there; the instant one lands first.
    play_party(
        &mut k,
        &[
            (living[0], SLOW_REBIRTH, Some(dead[0])),
            (living[1], REBIRTH, Some(dead[1])),
        ],
        t(5000),
    );
    assert!(alive(&k, dead[1]));
    assert!(!alive(&k, dead[0]));
    assert!(!k.state().res_offered(Seat(dead[0] as u8)));
    let pool = k.state().battle_res();
    assert_eq!(pool.charges, 0);
    assert_eq!(pool.next_at, Some(t(2000) + SimDuration(30_000)));
}

#[test]
fn a_soulstone_waits_for_a_charge_and_is_used_up() {
    let mut f = fixture(Vec::new());
    f.seats = 3;
    f.externals.battle_res = BattleRes {
        initial: 1,
        every: SimDuration(20_000),
        max: 5,
    };
    let mut stone = aura(STONE);
    stone.persists_through_death = true;
    stone.self_resurrect = Some(0.6);
    f.data.auras.insert(STONE, stone);
    add_spell(
        &mut f,
        spell(STONE_UP, vec![apply(STONE, EffectTarget::Caster)]),
    );
    rebirths(&mut f);
    lethal(&mut f, EnemyTarget::RandomPlayers(2));
    let max = f.seat_max_health();
    let mut k = f.kernel();
    play_party(
        &mut k,
        &[
            (0, STONE_UP, None),
            (1, STONE_UP, None),
            (2, STONE_UP, None),
        ],
        t(1500),
    );
    play_party(&mut k, &[], SimTime(15_000));
    let mut trace = k.drain_trace();
    let (dead, living) = dead_and_living(&k, &trace);
    assert_eq!(dead.len(), 2, "{dead:?}");
    // The one waiting on its stone is no one else's to resurrect.
    assert!(k.state().res_offered(Seat(dead[1] as u8)));
    assert_eq!(readiness(&k, living[0], REBIRTH), Some(Readiness::Blocked));
    play_party(&mut k, &[], SimTime(25_000));
    trace.extend(k.drain_trace());
    let seats = k.state().seats().to_vec();
    // One takes the charge at once; the other waits for the next, 20 s
    // into the run.
    let back = resurrections(&trace);
    assert_eq!(
        back,
        [(t(2000), seats[dead[0]]), (SimTime(20_000), seats[dead[1]])]
    );
    let has_stone = |i: usize| k.state().auras(seats[i]).iter().any(|a| a.aura == STONE);
    for &d in &dead {
        let a = k.state().actor(seats[d]).unwrap();
        assert!(a.alive);
        assert_eq!(a.health, (max as f64 * 0.6).round() as u64);
        assert!(!has_stone(d));
    }
    assert!(has_stone(living[0]));
    let pool = k.state().battle_res();
    assert_eq!(pool.charges, 0);
    assert_eq!(pool.next_at, Some(SimTime(40_000)));
}
