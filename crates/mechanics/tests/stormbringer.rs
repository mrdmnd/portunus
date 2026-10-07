//! Stormkeeper and the Stormbringer hero tree, from the checked-in data,
//! through the real mechanics and kernel.

mod common;

use common::{
    aura, completed, fixture, holds, play_until, Fixture, Stacks, EARTH_SHOCK, LIGHTNING_BOLT,
};
use portunus_core::{AuraId, SimDuration, SimTime, SpellId, TalentId};
use portunus_engine::trace::{CastEndReason, TraceEvent};
use portunus_engine::{Engine, StateView, TraceRecord};
use portunus_gamedata::effect::{Effect, EffectTarget, ListenFor, Listener, ProcChance};
use portunus_gamedata::stats::{RatedStat, ResourceKind};
use portunus_ingest::{check_game_data, DataIssue, Owner};
use portunus_mechanics::CombatMath;

const TEMPEST: SpellId = SpellId(452201);
const LIGHTNING_BOLT_OVERLOAD: SpellId = SpellId(45284);
const STORMKEEPER_BUFF: AuraId = AuraId(191634);
const TEMPEST_BUFF: AuraId = AuraId(454015);
const UNLIMITED_POWER_BUFF: AuraId = AuraId(454394);
const STORM_SWELL_BUFF: AuraId = AuraId(455089);
const SUPERCHARGE: TalentId = TalentId(455110);
const STORM_SWELL: TalentId = TalentId(455088);

const SPEND_COUNTER: AuraId = AuraId(900_001);
const SPEND_MARKER: AuraId = AuraId(900_002);

/// Unlimited Power's stacks last 15 s each.
const UNLIMITED_POWER_DURATION: SimDuration = SimDuration(15_000);

fn stormbringer() -> Fixture {
    fixture("elemental_stormbringer.ron", |_| {})
}

/// Draws one card per Maelstrom spent from `deck`, applying a marker on a
/// success. Earth Shock is the only spender.
fn with_spend_deck(f: &mut Fixture, successes: u16, size: u16) {
    let mut counter = aura(SPEND_COUNTER, None);
    counter.listeners = vec![Listener {
        on: ListenFor::ResourceSpent(ResourceKind::Maelstrom),
        chance: ProcChance::Deck { successes, size },
        internal_cooldown: None,
        per_unit: true,
        shared_with: None,
        condition: None,
        effects: vec![Effect::ApplyAura {
            aura: SPEND_MARKER,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
        }],
    }];
    f.data.auras.insert(counter.id, counter);
    f.data
        .auras
        .insert(SPEND_MARKER, aura(SPEND_MARKER, Some(SimDuration(500))));
    f.template.passive_auras.push(SPEND_COUNTER);
}

/// How many markers each Earth Shock applied, in order.
fn markers_per_spend(trace: &[TraceRecord]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for r in trace {
        match r.event {
            TraceEvent::CastEnd {
                spell,
                reason: CastEndReason::Completed,
                ..
            } if spell == EARTH_SHOCK => out.push(0),
            TraceEvent::AuraApplied { aura, .. } if aura == SPEND_MARKER => {
                *out.last_mut().expect("only Earth Shock spends") += 1;
            }
            _ => {}
        }
    }
    out
}

#[test]
fn per_unit_decks_draw_a_card_per_point_and_proc_once() {
    // 60 Maelstrom per Earth Shock draws a whole 60-card deck each time.
    for successes in [1, 2] {
        let mut f = fixture("elemental.ron", |_| {});
        with_spend_deck(&mut f, successes, 60);
        for seed in 0..3 {
            let per = markers_per_spend(&f.rollout(seed));
            assert!(per.len() >= 5, "{} spends", per.len());
            assert!(per.iter().all(|&n| n == 1), "{successes} per deck: {per:?}");
        }
    }

    // A 120-card deck lasts exactly two spends.
    let mut f = fixture("elemental.ron", |_| {});
    with_spend_deck(&mut f, 1, 120);
    for seed in 0..3 {
        let per = markers_per_spend(&f.rollout(seed));
        for pair in per.as_chunks::<2>().0 {
            assert_eq!(pair.iter().sum::<usize>(), 1, "{per:?}");
        }
    }

    let mut f = fixture("elemental.ron", |_| {});
    with_spend_deck(&mut f, 1, 60);
    f.data.auras.get_mut(&SPEND_COUNTER).unwrap().listeners[0].on = ListenFor::DamageTaken;
    assert!(check_game_data(&f.data)
        .contains(&DataIssue::PerUnitWithoutSpend(Owner::Aura(SPEND_COUNTER))));
}

#[test]
fn tempest_replaces_bolt_while_its_stacks_last() {
    let f = stormbringer();
    let mut tempests = 0;
    for seed in 0..10 {
        let trace = f.rollout(seed);
        let spends = completed(&trace, EARTH_SHOCK);
        let mut stacks = Stacks::default();
        for r in &trace {
            match r.event {
                TraceEvent::CastStart { actor, spell, .. } => {
                    let held = stacks.get(actor, TEMPEST_BUFF);
                    if spell == TEMPEST {
                        assert!(held > 0, "Tempest without the buff at {:?}", r.time);
                        tempests += 1;
                    } else if spell == LIGHTNING_BOLT {
                        assert_eq!(held, 0, "Bolt with Tempest ready at {:?}", r.time);
                    }
                }
                TraceEvent::AuraApplied {
                    holder,
                    aura,
                    stacks: now,
                } if aura == TEMPEST_BUFF && now > stacks.get(holder, aura) => {
                    assert!(spends.contains(&r.time), "gained off a spend: {:?}", r.time);
                }
                _ => {}
            }
            stacks.apply(&r.event);
        }
    }
    assert!(tempests >= 10, "{tempests} Tempests");
}

#[test]
fn tempest_splashes_secondary_targets_for_65_percent() {
    let mut f = stormbringer();
    f.scenario.pulls[0].waves[0].mobs[0].1 = 3;
    let mut checked = 0;
    for seed in 0..4 {
        let trace = f.rollout(seed);
        let mut target = None;
        let mut primary: Option<u64> = None;
        for r in &trace {
            match &r.event {
                TraceEvent::CastStart {
                    spell, target: t, ..
                } if *spell == TEMPEST => {
                    target = *t;
                    primary = None;
                }
                TraceEvent::Damage(d) if d.spell == Some(TEMPEST) && !d.crit => {
                    if Some(d.target) == target {
                        primary = Some(d.amount);
                    } else if let Some(p) = primary {
                        let want = p as f64 * 0.65;
                        assert!((d.amount as f64 - want).abs() <= 1.0, "{} vs {p}", d.amount);
                        checked += 1;
                    }
                }
                _ => {}
            }
        }
    }
    assert!(checked >= 5, "{checked} splash hits checked");
}

#[test]
fn stormkeeper_bolts_are_instant_and_hit_harder() {
    let f = stormbringer();
    let mut hard: Vec<u64> = Vec::new();
    let mut charged: Vec<u64> = Vec::new();
    let mut plain_overloads: Vec<u64> = Vec::new();
    let mut charged_overloads: Vec<u64> = Vec::new();
    let (mut charged_bolts, mut overloads_of_charged) = (0, 0);
    let mut arc_discharges = 0;
    for seed in 0..5 {
        let trace = f.rollout(seed);
        let tempests = completed(&trace, TEMPEST);
        let mut stacks = Stacks::default();
        let mut started: Option<(SimTime, bool)> = None;
        // Bolts in flight, by whether Stormkeeper charged them as they went.
        let mut flying: std::collections::VecDeque<bool> = std::collections::VecDeque::new();
        let mut overloads: std::collections::VecDeque<bool> = std::collections::VecDeque::new();
        let mut last_bolt_charged = false;
        for r in &trace {
            match r.event {
                TraceEvent::CastStart { actor, spell, .. } if spell == LIGHTNING_BOLT => {
                    started = Some((r.time, stacks.get(actor, STORMKEEPER_BUFF) > 0));
                }
                TraceEvent::CastEnd {
                    spell,
                    reason: CastEndReason::Completed,
                    ..
                } if spell == LIGHTNING_BOLT => {
                    let (at, keeper) = started.take().expect("a started Bolt");
                    assert_eq!(at == r.time, keeper, "instant exactly when charged");
                    flying.push_back(keeper);
                    last_bolt_charged = keeper;
                    charged_bolts += usize::from(keeper);
                }
                TraceEvent::CastEnd {
                    spell,
                    reason: CastEndReason::Completed,
                    ..
                } if spell == LIGHTNING_BOLT_OVERLOAD => {
                    overloads.push_back(last_bolt_charged);
                    overloads_of_charged += usize::from(last_bolt_charged);
                }
                TraceEvent::Damage(ref d) if d.spell == Some(LIGHTNING_BOLT_OVERLOAD) => {
                    let keeper = overloads.pop_front().expect("an overload in flight");
                    if !d.crit {
                        if keeper {
                            &mut charged_overloads
                        } else {
                            &mut plain_overloads
                        }
                        .push(d.amount);
                    }
                }
                TraceEvent::Damage(ref d) if d.spell == Some(LIGHTNING_BOLT) => {
                    let keeper = flying.pop_front().expect("a Bolt in flight");
                    if !d.crit {
                        if keeper { &mut charged } else { &mut hard }.push(d.amount);
                    }
                }
                TraceEvent::AuraApplied {
                    holder,
                    aura,
                    stacks: now,
                } if aura == STORMKEEPER_BUFF
                    && now > stacks.get(holder, aura)
                    && tempests.contains(&r.time) =>
                {
                    arc_discharges += 1;
                }
                _ => {}
            }
            stacks.apply(&r.event);
        }
    }
    hard.sort_unstable();
    hard.dedup();
    charged.sort_unstable();
    charged.dedup();
    let ([normal], [keeper]) = (hard.as_slice(), charged.as_slice()) else {
        panic!("one hit size each: {hard:?} {charged:?}");
    };
    assert!(
        (*keeper as f64 - 2.5 * *normal as f64).abs() <= 2.0,
        "{keeper} vs {normal}"
    );
    assert!(arc_discharges > 0, "Tempest grants Stormkeeper charges");

    // Every charged Bolt overloads, and the overload is rolled before the
    // Bolt spends its stack, so even the last stack's keeps the bonus.
    assert_eq!(overloads_of_charged, charged_bolts);
    for sizes in [&mut plain_overloads, &mut charged_overloads] {
        sizes.sort_unstable();
        sizes.dedup();
    }
    let ([normal], [keeper]) = (plain_overloads.as_slice(), charged_overloads.as_slice()) else {
        panic!("one overload size each: {plain_overloads:?} {charged_overloads:?}");
    };
    assert!(
        (*keeper as f64 - 2.5 * *normal as f64).abs() <= 2.0,
        "{keeper} vs {normal}"
    );
}

#[test]
fn unlimited_power_stacks_expire_on_their_own_timers() {
    let f = stormbringer();
    for seed in 0..3 {
        let trace = f.rollout(seed);
        let spends = completed(&trace, EARTH_SHOCK);
        let live = |t: SimTime| {
            spends
                .iter()
                .filter(|&&e| e <= t && t.saturating_since(e) < UNLIMITED_POWER_DURATION)
                .count()
                .min(15)
        };
        let mut held = Stacks::default();
        let mut drops = 0;
        for r in &trace {
            match r.event {
                TraceEvent::AuraApplied {
                    holder,
                    aura,
                    stacks,
                } if aura == UNLIMITED_POWER_BUFF => {
                    assert_eq!(usize::from(stacks), live(r.time), "at {:?}", r.time);
                    drops += usize::from(stacks < held.get(holder, aura));
                }
                TraceEvent::AuraRemoved { aura, .. } if aura == UNLIMITED_POWER_BUFF => {
                    assert_eq!(live(r.time), 0, "at {:?}", r.time);
                }
                _ => {}
            }
            held.apply(&r.event);
        }
        assert!(spends.len() >= 5, "{} spends", spends.len());
        assert!(drops > 0, "a stack dropped while others stayed");
    }

    let mut k = f.kernel(1);
    assert!(play_until(&mut k, |k| holds(k, UNLIMITED_POWER_BUFF) >= 2).is_none());
    let m = f.mechanics(1);
    let state = k.state();
    let me = state.seats()[0];
    let rating = 1.0 + m.math().derived(state, me).haste_pct / 100.0;
    let stacks = f64::from(holds(&k, UNLIMITED_POWER_BUFF));
    let want = rating * (1.0 + stacks / 100.0);
    assert!((m.math().haste_mult(state, me) - want).abs() < 1e-9);
}

#[test]
fn storm_swell_adds_mastery_points() {
    let f = fixture("elemental_stormbringer.ron", |l| {
        l.talents.0.remove(&SUPERCHARGE);
        l.talents.0.insert(STORM_SWELL, 1);
    });
    let m = f.mechanics(2);
    let mut found = false;
    for seed in 0..10 {
        let mut k = f.kernel(seed);
        let me = k.state().seats()[0];
        let before = m.math().rated_pct(k.state(), me, RatedStat::Mastery);
        if play_until(&mut k, |k| holds(k, STORM_SWELL_BUFF) > 0).is_none() {
            let after = m.math().rated_pct(k.state(), me, RatedStat::Mastery);
            let coef = f.data.specs[&f.template.spec].mastery_coef;
            assert!(
                (after - before - 5.0 * coef).abs() < 1e-9,
                "{before} -> {after}"
            );
            found = true;
            break;
        }
    }
    assert!(found, "a Tempest landed Storm Swell");
}
