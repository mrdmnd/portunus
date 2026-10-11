//! Group play: what one seat's buffs do to another's numbers, on made-up
//! spells grafted onto the checked-in Elemental data.

mod synthetic;

use portunus_core::{
    ActorId, AuraId, Dist, EventName, Seat, SimDuration, SimTime, SpellId, Trigger,
};
use portunus_engine::{
    CastOpts, Choice, Engine, Kernel, Readiness, StateView, Step, TargetSel, Wait,
};
use portunus_gamedata::effect::{EffectTarget, ModKind, ModScope, Modifier};
use portunus_gamedata::enemy::{EnemyAction, EnemyRule, EnemyTarget};
use portunus_gamedata::spell::Targeting;
use portunus_gamedata::stats::{SchoolMask, Stat};
use portunus_mechanics::PartyMechanics;
use synthetic::*;

const FORTITUDE: AuraId = AuraId(900_951);
const FORTIFY: SpellId = SpellId(900_952);

fn health(k: &Kernel<PartyMechanics>) -> (u64, u64) {
    let a = k.state().actor(ActorId(0)).unwrap();
    (a.health, a.max_health)
}

#[test]
fn stamina_buffs_change_max_health_and_keep_the_fraction_of_it() {
    let mut f = fixture(Vec::new());
    let max = f.seat_max_health();
    let mut fort = aura(FORTITUDE);
    fort.duration = Some(SimDuration(5000));
    fort.modifiers = vec![Modifier {
        scope: ModScope::All,
        kind: ModKind::StatPct(Stat::Stamina),
        value: 5.0,
        per_stack: false,
        condition: None,
    }];
    f.data.auras.insert(FORTITUDE, fort);
    add_spell(
        &mut f,
        spell(FORTIFY, vec![apply(FORTITUDE, EffectTarget::Caster)]),
    );
    // A hit for 40% of the seat's health as the boss engages.
    f.boss(vec![EnemyRule {
        name: EventName("hit".into()),
        phase: None,
        when: Trigger::Now,
        repeat: None,
        action: EnemyAction::Damage {
            amount: Dist::Fixed(0.4 * max as f64),
            school: SchoolMask::FIRE,
            target: EnemyTarget::AllPlayers,
        },
    }]);
    let mut k = f.kernel();
    play(&mut k, &[], COMBAT_START + SimDuration(500));
    let (hurt, before) = health(&k);
    assert_eq!(before, max);
    assert!(hurt < max, "{hurt}");
    play(&mut k, &[(FORTIFY, 0)], COMBAT_START + SimDuration(1000));
    let (now, raised) = health(&k);
    assert_eq!(raised, (max as f64 * 1.05).round() as u64);
    let frac = |h: u64, m: u64| h as f64 / m as f64;
    assert!((frac(now, raised) - frac(hurt, max)).abs() < 1e-3);
    play(&mut k, &[], COMBAT_START + SimDuration(7000));
    let (after, back) = health(&k);
    assert_eq!(back, max);
    assert!((frac(after, back) - frac(hurt, max)).abs() < 1e-3);
}

const EBON: AuraId = AuraId(900_953);
const EBON_MIGHT: SpellId = SpellId(900_954);
const BULK: AuraId = AuraId(900_955);
const BULK_UP: SpellId = SpellId(900_956);

fn max_health(k: &Kernel<PartyMechanics>, seat: usize) -> u64 {
    let s = k.state();
    s.actor(s.seats()[seat]).unwrap().max_health
}

/// Cast each `(caster, spell, target seat)` in turn once the caster can,
/// then wait until `until`; no seat means the primary target.
fn play_party(
    k: &mut Kernel<PartyMechanics>,
    script: &[(usize, SpellId, Option<usize>)],
    until: SimTime,
) {
    let mut next = 0;
    loop {
        match k.advance().unwrap() {
            Step::Done(_) => return,
            Step::Decide(req) => {
                if req.now >= until {
                    return;
                }
                let choice = match script.get(next) {
                    Some(&(caster, spell, target))
                        if req.seat == Seat(caster as u8)
                            && req.now >= COMBAT_START
                            && k.legal(req.seat).abilities.get(&spell) == Some(&Readiness::Now) =>
                    {
                        next += 1;
                        Choice::Cast {
                            ability: spell,
                            target: target.map_or(TargetSel::Primary, |t| {
                                TargetSel::Actor(k.state().seats()[t])
                            }),
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
}

#[test]
fn shared_stamina_follows_the_source_and_never_chains() {
    let mut f = fixture(Vec::new());
    f.seats = 2;
    let base = f.seat_max_health();
    let modifier = |kind, value| Modifier {
        scope: ModScope::All,
        kind,
        value,
        per_stack: false,
        condition: None,
    };
    let mut ebon = aura(EBON);
    ebon.modifiers = vec![modifier(ModKind::StatShare(Stat::Stamina), 50.0)];
    f.data.auras.insert(EBON, ebon);
    let mut share = spell(EBON_MIGHT, vec![apply(EBON, EffectTarget::Target)]);
    share.targeting = Targeting::Ally;
    share.hostile = false;
    add_spell(&mut f, share);
    let mut bulk = aura(BULK);
    bulk.duration = Some(SimDuration(5000));
    bulk.modifiers = vec![modifier(ModKind::StatPct(Stat::Stamina), 20.0)];
    f.data.auras.insert(BULK, bulk);
    add_spell(
        &mut f,
        spell(BULK_UP, vec![apply(BULK, EffectTarget::Caster)]),
    );
    let mut k = f.kernel();
    let t = |ms| COMBAT_START + SimDuration(ms);

    play_party(&mut k, &[(0, EBON_MIGHT, Some(1))], t(500));
    let shared = max_health(&k, 1);
    assert!(shared > base, "{shared} vs {base}");
    assert_eq!(max_health(&k, 0), base);

    // Sharing back gives seat 0 the same, and doesn't feed seat 1's share.
    play_party(&mut k, &[(1, EBON_MIGHT, Some(0))], t(1500));
    assert_eq!(max_health(&k, 0), shared);
    assert_eq!(max_health(&k, 1), shared);

    // Seat 0's own buff grows what it shares, and stops when it ends.
    play_party(&mut k, &[(0, BULK_UP, None)], t(2500));
    assert!(max_health(&k, 0) > shared);
    let grown = max_health(&k, 1);
    assert!(grown > shared, "{grown} vs {shared}");
    play_party(&mut k, &[], t(9000));
    assert_eq!(max_health(&k, 1), shared);
    assert_eq!(max_health(&k, 0), shared);
}
