//! Tertiary stats, stat conversions, highest-secondary procs and armor
//! penetration, on made-up auras grafted onto the checked-in Elemental
//! data.

mod synthetic;

use portunus_core::{ActorId, AuraId, Dist, EventName, Seat, SimDuration, SimTime, Trigger};
use portunus_engine::trace::{TraceEvent, TraceRecord};
use portunus_engine::{Engine, Kernel, StateView};
use portunus_gamedata::effect::{Effect, EffectTarget, ModKind, ModScope, Modifier};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemyTarget};
use portunus_gamedata::stats::{SchoolMask, Stat};
use portunus_mechanics::PartyMechanics;
use synthetic::*;

const BUFF: AuraId = AuraId(900_961);
const BUFFING: portunus_core::SpellId = portunus_core::SpellId(900_962);
const PICK: portunus_core::SpellId = portunus_core::SpellId(900_963);
const CRIT_UP: AuraId = AuraId(900_964);
const CRITTING: portunus_core::SpellId = portunus_core::SpellId(900_965);
const PICKED: [AuraId; 4] = [
    AuraId(900_966),
    AuraId(900_967),
    AuraId(900_968),
    AuraId(900_969),
];

fn t(ms: u32) -> SimTime {
    COMBAT_START + SimDuration(ms)
}

fn modifier(kind: ModKind, value: f64) -> Modifier {
    Modifier {
        scope: ModScope::All,
        kind,
        value,
        per_stack: false,
        condition: None,
    }
}

/// Held by the seat from the start.
fn passive(f: &mut Fixture, kind: ModKind, value: f64) {
    let kit = f.data.auras.get_mut(&KIT).unwrap();
    kit.modifiers.push(modifier(kind, value));
}

/// `BUFFING` gives the seat `BUFF` with these modifiers for 5 s.
fn timed_buff(f: &mut Fixture, mods: Vec<Modifier>) {
    let mut buff = aura(BUFF);
    buff.duration = Some(SimDuration(5000));
    buff.modifiers = mods;
    f.data.auras.insert(BUFF, buff);
    add_spell(f, spell(BUFFING, vec![apply(BUFF, EffectTarget::Caster)]));
}

fn hit_at_engage(amount: f64, target: EnemyTarget) -> EnemyRule {
    EnemyRule {
        name: EventName(format!("hit {target:?}")),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::Damage {
            amount: Dist::Fixed(amount),
            school: SchoolMask::FIRE,
            target,
        },
    }
}

fn seat(k: &Kernel<PartyMechanics>) -> ActorId {
    k.state().seats()[0]
}

fn health(k: &Kernel<PartyMechanics>) -> u64 {
    k.state().actor(seat(k)).unwrap().health
}

fn max_health(k: &Kernel<PartyMechanics>) -> u64 {
    k.state().actor(seat(k)).unwrap().max_health
}

/// Damage the seat took, in order.
fn taken(trace: &[TraceRecord], seat: ActorId) -> Vec<u64> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.target == seat => Some(d.amount),
            _ => None,
        })
        .collect()
}

#[test]
fn leech_heals_for_its_share_of_the_damage_dealt() {
    let mut f = fixture(Vec::new());
    // 5% Leech: 5250 rating at 1050 a percent, short of the first knee.
    passive(&mut f, ModKind::StatFlat(Stat::Leech), 5250.0);
    // 10% Versatility, which the heal gets too: 7800 rating at 780.
    passive(&mut f, ModKind::StatFlat(Stat::VersatilityRating), 7800.0);
    let max = f.seat_max_health();
    f.boss(vec![hit_at_engage(
        0.4 * max as f64,
        EnemyTarget::AllPlayers,
    )]);
    let mut k = f.kernel();
    play(&mut k, &[], t(500));
    let hurt = health(&k);
    assert!(hurt < max);
    k.drain_trace();
    play(&mut k, &[(STRIKE, 0)], t(1500));
    let trace = k.drain_trace();
    let boss = k.state().enemies()[0];
    let dealt = hits(&trace, STRIKE, boss);
    assert_eq!(dealt.len(), 1);
    let healed = health(&k) - hurt;
    assert!(healed > 0);
    assert!(
        close(healed, dealt[0] as f64 * 0.05 * 1.1),
        "{healed} of {dealt:?}"
    );
}

#[test]
fn avoidance_shrinks_only_hits_that_reach_several_players() {
    let mut f = fixture(Vec::new());
    // 10% Avoidance: 5600 rating at 560 a percent, right at the knee.
    passive(&mut f, ModKind::StatFlat(Stat::Avoidance), 5600.0);
    f.boss(vec![
        hit_at_engage(10_000.0, EnemyTarget::RandomPlayer),
        hit_at_engage(10_000.0, EnemyTarget::AllPlayers),
    ]);
    let mut k = f.kernel();
    play(&mut k, &[], t(500));
    let got = taken(&k.drain_trace(), seat(&k));
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(close(got[1], got[0] as f64 * 0.9), "{got:?}");
}

#[test]
fn speed_rating_adds_to_run_speed_while_it_lasts() {
    let mut f = fixture(Vec::new());
    timed_buff(
        &mut f,
        vec![modifier(ModKind::StatFlat(Stat::Speed), 1750.0)],
    );
    let mut k = f.kernel();
    play(&mut k, &[], t(500));
    let base = k.state().run_speed(Seat(0));
    play(&mut k, &[(BUFFING, 0)], t(1500));
    // 10% before diminishing returns, then SimC's falloff.
    let pct = 10.0 * 0.1 * (-0.0003_f64 * 1750.0).exp();
    let fast = k.state().run_speed(Seat(0));
    assert!(
        (fast - base * (1.0 + pct / 100.0)).abs() < 1e-9,
        "{fast} vs {base}"
    );
    play(&mut k, &[], t(7000));
    assert_eq!(k.state().run_speed(Seat(0)), base);
}

#[test]
fn conversions_add_after_multipliers_and_never_chain() {
    let mut f = fixture(Vec::new());
    let stat = |s| f.template.stats.0.get(&s).copied().unwrap_or(0.0);
    let (int, stam) = (stat(Stat::Intellect), stat(Stat::Stamina));
    assert!(stat(Stat::CritRating) > 0.0);
    // Listed first, so it would feed the next one if conversions chained.
    passive(
        &mut f,
        ModKind::StatFromStat {
            from: Stat::CritRating,
            to: Stat::Intellect,
        },
        100.0,
    );
    passive(
        &mut f,
        ModKind::StatFromStat {
            from: Stat::Intellect,
            to: Stat::Stamina,
        },
        10.0,
    );
    passive(&mut f, ModKind::StatPct(Stat::Stamina), 5.0);
    timed_buff(
        &mut f,
        vec![modifier(ModKind::StatFlat(Stat::Intellect), 1000.0)],
    );
    let per_stamina = f.data.curves.health_per_stamina;
    let mut k = f.kernel();
    play(&mut k, &[], t(500));
    let want = |int: f64| ((stam * 1.05 + int * 0.1 * 1.05) * per_stamina).round() as u64;
    assert_eq!(max_health(&k), want(int));
    // More Intellect reaches health through the conversion, and leaves.
    play(&mut k, &[(BUFFING, 0)], t(1500));
    assert_eq!(max_health(&k), want(int + 1000.0));
    play(&mut k, &[], t(7000));
    assert_eq!(max_health(&k), want(int));
}

#[test]
fn highest_secondary_picks_by_rating_now_with_ties_to_mastery_over_crit() {
    let mut f = fixture(Vec::new());
    let stat = |s| f.template.stats.0.get(&s).copied().unwrap_or(0.0);
    assert_eq!(stat(Stat::CritRating), stat(Stat::MasteryRating));
    for a in PICKED {
        f.data.auras.insert(a, aura(a));
    }
    let give = |a| vec![apply(a, EffectTarget::Caster)];
    add_spell(
        &mut f,
        spell(
            PICK,
            vec![Effect::HighestSecondary {
                crit: give(PICKED[0]),
                haste: give(PICKED[1]),
                mastery: give(PICKED[2]),
                versatility: give(PICKED[3]),
            }],
        ),
    );
    let mut crit = aura(CRIT_UP);
    crit.modifiers = vec![modifier(ModKind::StatFlat(Stat::CritRating), 1.0)];
    f.data.auras.insert(CRIT_UP, crit);
    add_spell(
        &mut f,
        spell(CRITTING, vec![apply(CRIT_UP, EffectTarget::Caster)]),
    );
    let mut k = f.kernel();
    let held = |k: &Kernel<PartyMechanics>| -> Vec<bool> {
        PICKED
            .iter()
            .map(|&a| k.state().auras(seat(k)).iter().any(|i| i.aura == a))
            .collect()
    };
    play(&mut k, &[(PICK, 0)], t(1500));
    assert_eq!(held(&k), [false, false, true, false], "tie goes to mastery");
    play(&mut k, &[(CRITTING, 0), (PICK, 0)], t(4000));
    assert_eq!(held(&k), [true, false, true, false], "one more crit wins");
}

#[test]
fn armor_penetration_multiplies_away_the_targets_armor() {
    const ARMOR: f64 = 10_000.0;
    let strike = |pens: &[f64]| {
        let mut f = fixture(Vec::new());
        for &p in pens {
            passive(&mut f, ModKind::ArmorPenPct, p);
        }
        f.enemies.enemies.get_mut(&dummy()).unwrap().defense.armor = ARMOR;
        let k_armor = f.data.curves.armor_constant;
        let mut k = f.kernel();
        play(&mut k, &[(STRIKE, 0)], t(1500));
        let boss = k.state().enemies()[0];
        let got = hits(&k.drain_trace(), STRIKE, boss);
        assert_eq!(got.len(), 1);
        (got[0], k_armor)
    };
    let (full, k_armor) = strike(&[]);
    let (pierced, _) = strike(&[30.0, 50.0]);
    let kept = |a: f64| 1.0 - a / (a + k_armor);
    let want = full as f64 * kept(ARMOR * 0.7 * 0.5) / kept(ARMOR);
    assert!((pierced as f64 - want).abs() <= 2.0, "{pierced} vs {want}");
}
