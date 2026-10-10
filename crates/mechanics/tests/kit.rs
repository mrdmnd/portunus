//! The rest of the Elemental kit, from the checked-in full builds, through
//! the real mechanics and kernel.

mod common;

use std::collections::BTreeMap;

use common::{completed, fixture, holds, play_until, priority, Fixture, LIGHTNING_BOLT};
use portunus_core::{ActorId, AuraId, PetId, Seat, SimDuration, SimTime, SpellId, TalentId};
use portunus_engine::trace::{CastEndReason, TraceEvent};
use portunus_engine::{
    CastOpts, Choice, Engine, Kernel, Readiness, StateView, Step, TargetSel, TraceRecord,
};
use portunus_gamedata::effect::{Effect, Listener, Predicate, ProcChance};
use portunus_ingest::{check_game_data, DataIssue, Owner};
use portunus_mechanics::PartyMechanics;

const VOLTAIC_BLAZE: SpellId = SpellId(470057);
const THUNDERSTRIKE_WARD: SpellId = SpellId(462757);
const THUNDERSTRIKE: SpellId = SpellId(462763);
const TEMPEST: SpellId = SpellId(452201);
const LIGHTNING_BOLT_OVERLOAD: SpellId = SpellId(45284);
const TEMPEST_OVERLOAD: SpellId = SpellId(463351);
const CRACKLING_FURY: AuraId = AuraId(1269215);
const THUNDERSTRIKE_WARD_BUFF: AuraId = AuraId(462757);
const WIND_GUST: AuraId = AuraId(263806);
const PRIMAL_STORM_ELEMENTAL: PetId = PetId(77942);
const GREATER_STORM_ELEMENTAL: PetId = PetId(77936);
const MASTERY: AuraId = AuraId(168534);
const ELEMENTAL_RESONANCE: TalentId = TalentId(1258895);
const THUNDERSTRIKE_WARD_TALENT: TalentId = TalentId(462757);

fn stormbringer() -> Fixture {
    fixture("elemental_stormbringer_build.ron", |_| {})
}

fn press(k: &mut Kernel<PartyMechanics>, spell: SpellId) {
    k.submit(
        Seat(0),
        Choice::Cast {
            ability: spell,
            target: TargetSel::Primary,
            opts: CastOpts::default(),
        },
    )
    .unwrap();
}

fn ready(k: &Kernel<PartyMechanics>, spell: SpellId) -> bool {
    k.legal(Seat(0)).abilities.get(&spell) == Some(&Readiness::Now)
}

/// Each guardian of this type: when it came and, if it timed out, left.
fn lifetimes(trace: &[TraceRecord], kind: PetId) -> BTreeMap<ActorId, (SimTime, Option<SimTime>)> {
    let mut out = BTreeMap::new();
    for r in trace {
        match r.event {
            TraceEvent::PetSummoned { pet, actor, .. } if pet == kind => {
                out.insert(actor, (r.time, None));
            }
            TraceEvent::PetExpired { actor } => {
                if let Some(life) = out.get_mut(&actor) {
                    life.1 = Some(r.time);
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn crackling_fury_stacks_per_rank_and_cuts_voltaic_blaze() {
    let f = stormbringer();
    let mut k = f.kernel(0);
    assert!(play_until(&mut k, |k| ready(k, VOLTAIC_BLAZE)).is_none());
    assert_eq!(holds(&k, CRACKLING_FURY), 2, "one stack per rank");
    press(&mut k, VOLTAIC_BLAZE);
    let now = k.state().now();
    let me = k.state().seats()[0];
    let cd = k.state().cooldown(me, VOLTAIC_BLAZE).unwrap();
    // 10 s, less 3 s per rank and Flames of the Cauldron's 1.5 s.
    assert_eq!(cd.next_charge_at, Some(now + SimDuration(2500)));
}

#[test]
fn storm_elementals_come_out_primal_and_last_longer() {
    let f = stormbringer();
    let mut summoned = 0;
    for seed in 0..3 {
        let trace = f.rollout(seed);
        assert!(lifetimes(&trace, GREATER_STORM_ELEMENTAL).is_empty());
        let lives = lifetimes(&trace, PRIMAL_STORM_ELEMENTAL);
        let left: Vec<SimTime> = lives.values().filter_map(|l| l.1).collect();
        for &(from, until) in lives.values() {
            if let Some(until) = until {
                assert_eq!(until, from + SimDuration(12_000), "Everlasting Elements");
            }
        }
        summoned += lives.len();

        // Wind Gust builds while the Elemental is out and goes with it.
        let mut most = 0;
        for r in &trace {
            match r.event {
                TraceEvent::AuraApplied { aura, stacks, .. } if aura == WIND_GUST => {
                    most = most.max(stacks);
                }
                TraceEvent::AuraRemoved { aura, .. } if aura == WIND_GUST => {
                    assert!(left.contains(&r.time), "Wind Gust left at {:?}", r.time);
                }
                _ => {}
            }
        }
        assert_eq!(most, 4, "seed {seed}");
    }
    assert!(summoned > 0, "Stormkeeper summons the Storm Elemental");
}

#[test]
fn thunderstrike_ward_calls_two_strikes_on_a_share_of_casts() {
    let f = fixture("elemental_stormbringer_build.ron", |l| {
        l.talents.0.remove(&ELEMENTAL_RESONANCE);
        l.talents.0.insert(THUNDERSTRIKE_WARD_TALENT, 1);
    });
    let (mut eligible, mut procs) = (0, 0);
    for seed in 0..8 {
        let mut k = f.kernel(seed);
        loop {
            match k.advance().unwrap() {
                Step::Done(_) => break,
                Step::Decide(req) => {
                    if holds(&k, THUNDERSTRIKE_WARD_BUFF) == 0 && ready(&k, THUNDERSTRIKE_WARD) {
                        press(&mut k, THUNDERSTRIKE_WARD);
                    } else {
                        let choice = priority(&k, req.seat);
                        k.submit(req.seat, choice).unwrap();
                    }
                }
            }
        }
        let mut strikes: BTreeMap<SimTime, usize> = BTreeMap::new();
        for r in k.drain_trace() {
            match r.event {
                TraceEvent::CastEnd {
                    spell,
                    reason: CastEndReason::Completed,
                    ..
                } if spell == LIGHTNING_BOLT || spell == TEMPEST => eligible += 1,
                TraceEvent::Damage(ref d) if d.spell == Some(THUNDERSTRIKE) => {
                    *strikes.entry(r.time).or_default() += 1;
                }
                _ => {}
            }
        }
        // Two casts can complete in the same moment, and the first strike
        // of the last pair can kill.
        let last = strikes.pop_last();
        assert!(strikes.values().all(|&n| n % 2 == 0), "{strikes:?}");
        procs += strikes.values().sum::<usize>() / 2 + last.map_or(0, |(_, n)| n.div_ceil(2));
    }
    let rate = procs as f64 / f64::from(eligible);
    assert!((0.2..0.4).contains(&rate), "{procs} of {eligible}");
}

#[test]
fn chances_and_listener_conditions_are_checked() {
    let mut f = stormbringer();
    let mastery = f.data.auras.get_mut(&MASTERY).unwrap();
    mastery.listeners[0].condition = Some(Predicate::CasterLacksAura(AuraId(900_001)));
    mastery.listeners[1].effects = vec![Effect::Chance {
        chance: 1.5,
        then: Vec::new(),
    }];
    let issues = check_game_data(&f.data);
    assert!(issues.contains(&DataIssue::UnknownAura {
        owner: Owner::Aura(MASTERY),
        aura: AuraId(900_001),
    }));
    assert!(issues.contains(&DataIssue::InvalidChance(Owner::Aura(MASTERY))));
}

#[test]
fn missiles_fly_by_distance_and_overloads_wait_for_their_spell() {
    let mut f = stormbringer();
    f.scenario.pulls[0].waves[0].distance = 40.0;
    let tempest_overloads = |l: &Listener| {
        l.effects
            .iter()
            .any(|e| matches!(e, Effect::TriggerSpell { spell, .. } if *spell == TEMPEST_OVERLOAD))
    };
    let mastery = f.data.auras.get_mut(&MASTERY).unwrap();
    for l in mastery
        .listeners
        .iter_mut()
        .filter(|l| tempest_overloads(l))
    {
        l.chance = ProcChance::Always;
    }
    let mut flights: BTreeMap<SpellId, Vec<u32>> = BTreeMap::new();
    let mut overloaded = 0;
    for seed in 0..5 {
        let trace = f.rollout(seed);
        for r in &trace {
            if let TraceEvent::ProjectileLaunched { spell, lands, .. } = r.event {
                flights
                    .entry(spell)
                    .or_default()
                    .push(lands.saturating_since(r.time).millis());
            }
        }
        let bolts = completed(&trace, LIGHTNING_BOLT);
        for at in completed(&trace, LIGHTNING_BOLT_OVERLOAD) {
            assert!(
                bolts.contains(&(at - SimDuration(400))),
                "overload at {at:?} without a Bolt 400 ms before"
            );
            overloaded += 1;
        }
    }
    assert!(overloaded > 0);
    let only = |spell: SpellId| {
        let mut times = flights.get(&spell).cloned().unwrap_or_default();
        times.sort_unstable();
        times.dedup();
        times
    };
    // 40 yards at 60 and 50 yd/s.
    assert_eq!(only(LIGHTNING_BOLT), vec![667]);
    assert_eq!(only(LIGHTNING_BOLT_OVERLOAD), vec![800]);
    // Tempest's overload doesn't travel: its pre-rolled hits land as it
    // goes out.
    assert_eq!(only(TEMPEST_OVERLOAD), vec![0]);
}
