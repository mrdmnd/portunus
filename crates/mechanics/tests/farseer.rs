//! The Farseer hero tree, from the checked-in data, through the real
//! mechanics and kernel.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::{
    completed, fixture, holds, play_until, Fixture, Stacks, ANCESTRAL_SWIFTNESS, EARTH_SHOCK,
    FLAME_SHOCK, LAVA_BURST, LIGHTNING_BOLT, STORMKEEPER,
};
use portunus_core::{ActorId, AuraId, PetId, Seat, SimDuration, SimTime, SpellId, TalentId};
use portunus_engine::trace::{CastEndReason, TraceEvent};
use portunus_engine::{
    CastOpts, Choice, Engine, Kernel, Readiness, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::effect::{Effect, EffectTarget, ProcChance};
use portunus_gamedata::stats::ResourceKind;
use portunus_ingest::{check_game_data, DataIssue, Owner};
use portunus_mechanics::{CombatMath, PartyMechanics};

const ANCESTOR: PetId = PetId(221177);
const CALL_OF_THE_ANCESTORS: SpellId = SpellId(445624);
const ANCESTOR_LAVA_BURST: SpellId = SpellId(447419);
const FINAL_CALLING_BLAST: SpellId = SpellId(465717);
const ANCESTORS_BUFF: AuraId = AuraId(447244);
const ANCESTRAL_SWIFTNESS_BUFF: AuraId = AuraId(443454);
const ANCESTRAL_INFLUENCE: TalentId = TalentId(1270446);
const ROUTINE_COMMUNICATION: TalentId = TalentId(443445);
const HEED_MY_CALL: TalentId = TalentId(443444);
const FINAL_CALLING: TalentId = TalentId(443446);
const ROUTINE_COMMUNICATION_AURA: AuraId = AuraId(443445);
const STORMKEEPER_BUFF: AuraId = AuraId(191634);
const LAVA_SURGE_BUFF: AuraId = AuraId(77762);
const ELEMENTAL_BLAST_BUFFS: [AuraId; 3] = [AuraId(118522), AuraId(173183), AuraId(173184)];

/// The seat's own actor in every fixture run.
const OWNER: ActorId = ActorId(0);

fn farseer() -> Fixture {
    fixture("elemental_farseer.ron", |_| {})
}

/// Each Ancestor's arrival and departure, by actor.
fn lifetimes(trace: &[TraceRecord]) -> BTreeMap<ActorId, (SimTime, Option<SimTime>)> {
    let mut out = BTreeMap::new();
    for r in trace {
        match r.event {
            TraceEvent::PetSummoned { pet, actor, .. } if pet == ANCESTOR => {
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

fn completed_by(r: &TraceRecord, who: ActorId, spells: &[SpellId]) -> bool {
    matches!(
        r.event,
        TraceEvent::CastEnd {
            actor,
            spell,
            reason: CastEndReason::Completed,
        } if actor == who && spells.contains(&spell)
    )
}

#[test]
fn every_ancestor_echoes_each_cast_as_it_lands() {
    let f = farseer();
    let mut echoes = 0;
    for seed in 0..6 {
        let trace = f.rollout(seed);
        let mut live: BTreeSet<ActorId> = BTreeSet::new();
        let (mut want, mut got) = (0, 0);
        for r in &trace {
            match r.event {
                TraceEvent::PetSummoned { pet, actor, .. } if pet == ANCESTOR => {
                    live.insert(actor);
                }
                TraceEvent::PetExpired { actor } => {
                    live.remove(&actor);
                }
                TraceEvent::Damage(d)
                    if d.source == OWNER
                        && matches!(d.spell, Some(LAVA_BURST | LIGHTNING_BOLT)) =>
                {
                    want += live.len();
                }
                TraceEvent::CastEnd {
                    actor,
                    spell,
                    reason: CastEndReason::Completed,
                } if spell == ANCESTOR_LAVA_BURST => {
                    assert!(live.contains(&actor), "an echo from a departed Ancestor");
                    got += 1;
                }
                _ if completed_by(r, OWNER, &[FLAME_SHOCK, EARTH_SHOCK]) => {
                    want += live.len();
                }
                _ => {}
            }
        }
        assert_eq!(got, want, "seed {seed}");
        echoes += got;
    }
    assert!(echoes >= 50, "{echoes} echoes");
}

#[test]
fn ancestor_lava_burst_always_crits_for_one_plus_crit_chance() {
    // Without Ancestral Influence or Final Calling's Elemental Blast buffs
    // the Ancestors' power never changes.
    let f = fixture("elemental_farseer.ron", |l| {
        l.talents.0.remove(&ANCESTRAL_INFLUENCE);
        l.talents.0.remove(&FINAL_CALLING);
    });
    let mut k = f.kernel(4);
    assert!(play_until(&mut k, |k| !k.state().pets(Seat(0)).is_empty()).is_none());
    let m = f.mechanics(4);
    let pet = k.state().pets(Seat(0))[0];
    let d = m.math().derived(k.state(), pet);
    // Elemental Fury's +15% crit damage reaches the Ancestor's spells.
    let want = 1.1775
        * d.spell_power
        * (1.0 + d.versatility_pct / 100.0)
        * 2.0
        * 1.15
        * (1.0 + d.crit_pct / 100.0);
    assert!(d.crit_pct > 0.0);

    let trace = f.rollout(4);
    let hits: Vec<_> = trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.spell == Some(ANCESTOR_LAVA_BURST) => Some(d),
            _ => None,
        })
        .collect();
    assert!(hits.len() >= 5, "{} hits", hits.len());
    for hit in hits {
        assert!(hit.crit);
        assert!(
            (hit.amount as f64 - want).abs() <= 1.0,
            "{} vs {want}",
            hit.amount
        );
    }
}

#[test]
fn departures_cast_final_calling_and_sometimes_call_another() {
    let f = farseer();
    let mut fellowship = 0;
    let mut buffs = BTreeSet::new();
    for seed in 0..10 {
        let trace = f.rollout(seed);
        let lives = lifetimes(&trace);
        let mut departures: Vec<(SimTime, ActorId)> = lives
            .iter()
            .filter_map(|(&actor, l)| l.1.map(|t| (t, actor)))
            .collect();
        departures.sort();
        let departed: Vec<SimTime> = departures.iter().map(|d| d.0).collect();
        // Each departing Ancestor casts the blast itself, as it leaves.
        let mut blasts: Vec<(SimTime, ActorId)> = trace
            .iter()
            .filter_map(|r| match r.event {
                TraceEvent::CastEnd {
                    actor,
                    spell,
                    reason: CastEndReason::Completed,
                } if spell == FINAL_CALLING_BLAST => Some((r.time, actor)),
                _ => None,
            })
            .collect();
        blasts.sort();
        assert_eq!(
            blasts, departures,
            "one Elemental Blast per departing Ancestor"
        );
        let blast_hits = trace.iter().filter(
            |r| matches!(r.event, TraceEvent::Damage(d) if d.spell == Some(FINAL_CALLING_BLAST)),
        );
        for r in blast_hits {
            let TraceEvent::Damage(d) = r.event else {
                unreachable!()
            };
            assert!(lives.contains_key(&d.source), "the Ancestor's blast");
        }

        // Each departure grants the owner an Elemental Blast buff, a new
        // one while any is missing.
        let mut stacks = Stacks::default();
        let mut next = departed.iter().peekable();
        for r in &trace {
            while next.peek().is_some_and(|&&t| t < r.time) {
                let held = ELEMENTAL_BLAST_BUFFS
                    .iter()
                    .filter(|&&a| stacks.get(OWNER, a) > 0)
                    .count();
                assert!(held > 0, "a buff after the departure");
                next.next();
            }
            if let TraceEvent::AuraApplied { holder, aura, .. } = r.event {
                if holder == OWNER && ELEMENTAL_BLAST_BUFFS.contains(&aura) {
                    let held = ELEMENTAL_BLAST_BUFFS
                        .iter()
                        .filter(|&&a| stacks.get(OWNER, a) > 0)
                        .count();
                    assert!(
                        stacks.get(OWNER, aura) == 0 || held == 3,
                        "{aura:?} repeated with a buff missing"
                    );
                    buffs.insert(aura);
                }
            }
            stacks.apply(&r.event);
        }

        let stormkeepers = completed(&trace, STORMKEEPER);
        let routine: BTreeSet<SimTime> = trace
            .iter()
            .filter(|r| completed_by(r, OWNER, &[LIGHTNING_BOLT, LAVA_BURST, FLAME_SHOCK]))
            .map(|r| r.time)
            .collect();
        for call in completed(&trace, CALL_OF_THE_ANCESTORS) {
            let by_departure = departed.contains(&call);
            assert!(
                stormkeepers.contains(&call) || routine.contains(&call) || by_departure,
                "an unexplained call at {call:?}"
            );
            fellowship += usize::from(
                by_departure && !stormkeepers.contains(&call) && !routine.contains(&call),
            );
        }
    }
    assert!(fellowship > 0, "Ancient Fellowship called an Ancestor");
    assert_eq!(buffs.len(), 3, "every Elemental Blast buff came up");
}

#[test]
fn routine_communication_draws_every_spell_from_one_deck() {
    let f = farseer();
    for seed in 0..4 {
        let mut k = f.kernel(seed);
        let stop = SimTime(60_000);
        assert!(play_until(&mut k, |k| k.state().now() >= stop).is_none());
        let trace = k.drain_trace();
        let draws = trace
            .iter()
            .filter(|r| completed_by(r, OWNER, &[LIGHTNING_BOLT, LAVA_BURST, FLAME_SHOCK]))
            .count();
        assert!(draws > 10, "{draws} draws");
        let decks: Vec<_> = k
            .state()
            .procs(OWNER)
            .iter()
            .filter(|p| p.listener.aura.aura == ROUTINE_COMMUNICATION_AURA && p.deck.is_some())
            .collect();
        let [only] = decks.as_slice() else {
            panic!("{} decks", decks.len());
        };
        assert_eq!(only.listener.index, 0);
        let deck = only.deck.expect("a deck");
        assert_eq!(usize::from(50 - deck.cards) % 50, draws % 50, "seed {seed}");
    }
}

type FarseerKernel = Kernel<PartyMechanics>;

fn press(k: &mut FarseerKernel, spell: SpellId) {
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

/// Wait, without acting, until the seat may cast `spell`, then cast it.
fn press_when_ready(k: &mut FarseerKernel, spell: SpellId) {
    loop {
        let Step::Decide(_) = k.advance().unwrap() else {
            panic!("ended before {spell:?} was ready");
        };
        if k.legal(Seat(0)).abilities.get(&spell) == Some(&Readiness::Now) {
            return press(k, spell);
        }
        k.submit(Seat(0), Choice::Wait(Wait::NextEvent)).unwrap();
    }
}

fn ready(k: &FarseerKernel, spell: SpellId) -> bool {
    k.legal(Seat(0)).abilities.get(&spell) == Some(&Readiness::Now)
}

#[test]
fn ancestral_swiftness_cools_down_from_when_it_is_spent() {
    let f = farseer();
    let mut k = f.kernel(2);
    let fresh = |k: &FarseerKernel| {
        ready(k, ANCESTRAL_SWIFTNESS) && ready(k, LAVA_BURST) && ready(k, FLAME_SHOCK)
    };
    assert!(play_until(&mut k, fresh).is_none());
    press(&mut k, ANCESTRAL_SWIFTNESS);
    let cast_at = k.state().now();
    press_when_ready(&mut k, FLAME_SHOCK);
    assert_eq!(
        holds(&k, ANCESTRAL_SWIFTNESS_BUFF),
        1,
        "Flame Shock leaves it"
    );
    press_when_ready(&mut k, LAVA_BURST);
    let spent_at = k.state().now();
    assert!(spent_at > cast_at);
    assert_eq!(holds(&k, ANCESTRAL_SWIFTNESS_BUFF), 0);
    let cd = k.state().cooldown(OWNER, ANCESTRAL_SWIFTNESS).unwrap();
    assert_eq!(cd.next_charge_at, Some(spent_at + SimDuration(30_000)));
}

#[test]
fn stormkeeper_bolts_leave_ancestral_swiftness_up() {
    let f = farseer();
    // The Bolt takes Stormkeeper's last stack: SimC still leaves the buff.
    let last_stack = |k: &FarseerKernel| {
        holds(k, STORMKEEPER_BUFF) == 1
            && holds(k, ANCESTRAL_SWIFTNESS_BUFF) == 0
            && ready(k, ANCESTRAL_SWIFTNESS)
            && ready(k, LIGHTNING_BOLT)
    };
    let seed = (0..30)
        .find(|&s| play_until(&mut f.kernel(s), last_stack).is_none())
        .expect("a last Stormkeeper stack with Ancestral Swiftness ready");
    let mut k = f.kernel(seed);
    assert!(play_until(&mut k, last_stack).is_none());
    press(&mut k, ANCESTRAL_SWIFTNESS);
    press_when_ready(&mut k, LIGHTNING_BOLT);
    assert_eq!(holds(&k, STORMKEEPER_BUFF), 0);
    assert_eq!(holds(&k, ANCESTRAL_SWIFTNESS_BUFF), 1);
    press_when_ready(&mut k, LIGHTNING_BOLT);
    assert_eq!(
        holds(&k, ANCESTRAL_SWIFTNESS_BUFF),
        0,
        "an ordinary Bolt spends it"
    );
}

#[test]
fn lava_burst_spends_ancestral_swiftness_before_lava_surge() {
    let f = farseer();
    let both = |k: &FarseerKernel| {
        holds(k, LAVA_SURGE_BUFF) > 0 && ready(k, ANCESTRAL_SWIFTNESS) && ready(k, LAVA_BURST)
    };
    let seed = (0..30)
        .find(|&s| play_until(&mut f.kernel(s), both).is_none())
        .expect("Lava Surge with Ancestral Swiftness ready");
    let mut k = f.kernel(seed);
    assert!(play_until(&mut k, both).is_none());
    press(&mut k, ANCESTRAL_SWIFTNESS);
    press_when_ready(&mut k, LAVA_BURST);
    assert_eq!(holds(&k, ANCESTRAL_SWIFTNESS_BUFF), 0);
    assert_eq!(holds(&k, LAVA_SURGE_BUFF), 1, "Lava Surge kept");
    press_when_ready(&mut k, LAVA_BURST);
    assert_eq!(holds(&k, LAVA_SURGE_BUFF), 0);
}

#[test]
fn the_buff_counts_active_ancestors_and_raises_intellect() {
    let f = farseer();
    let mut checked = 0;
    for seed in 0..5 {
        let trace = f.rollout(seed);
        let lives = lifetimes(&trace);
        let changes: BTreeSet<SimTime> = lives
            .values()
            .flat_map(|&(from, to)| [Some(from), to])
            .flatten()
            .collect();
        let mut live: BTreeSet<ActorId> = BTreeSet::new();
        let mut stacks = Stacks::default();
        for r in &trace {
            match r.event {
                TraceEvent::PetSummoned { pet, actor, .. } if pet == ANCESTOR => {
                    live.insert(actor);
                }
                TraceEvent::PetExpired { actor } => {
                    live.remove(&actor);
                }
                TraceEvent::Damage(_) if !changes.contains(&r.time) => {
                    let held = stacks.get(OWNER, ANCESTORS_BUFF);
                    assert_eq!(usize::from(held), live.len(), "at {:?}", r.time);
                    checked += 1;
                }
                _ => {}
            }
            stacks.apply(&r.event);
        }
    }
    assert!(checked >= 100, "{checked} hits checked");

    let seed = (0..20)
        .find(|&s| {
            let mut k = f.kernel(s);
            play_until(&mut k, |k| holds(k, ANCESTORS_BUFF) >= 2).is_none()
        })
        .expect("two Ancestors at once");
    let mut k = f.kernel(seed);
    assert!(play_until(&mut k, |_| true).is_none());
    let m = f.mechanics(seed);
    let alone = m.math().derived(k.state(), OWNER).spell_power;
    assert!(play_until(&mut k, |k| holds(k, ANCESTORS_BUFF) >= 2).is_none());
    let n = f64::from(holds(&k, ANCESTORS_BUFF));
    let with = m.math().derived(k.state(), OWNER).spell_power;
    assert!(
        (with - alone * (1.0 + n / 100.0)).abs() < 1e-6,
        "{alone} -> {with}"
    );
}

#[test]
fn ancestors_last_8_seconds_or_12_with_heed_my_call() {
    let heeded = fixture("elemental_farseer.ron", |l| {
        l.talents.0.remove(&ROUTINE_COMMUNICATION);
        l.talents.0.insert(HEED_MY_CALL, 1);
    });
    for (f, want) in [(farseer(), 8_000), (heeded, 12_000)] {
        let mut full = 0;
        for seed in 0..4 {
            let trace = f.rollout(seed);
            for (from, to) in lifetimes(&trace).into_values() {
                if let Some(to) = to {
                    assert_eq!(to.saturating_since(from), SimDuration(want));
                    full += 1;
                }
            }
            let mut stacks = Stacks::default();
            for r in &trace {
                if let TraceEvent::AuraRemoved { holder, aura } = r.event {
                    if aura == ANCESTORS_BUFF && holder == OWNER {
                        assert!(stacks.get(holder, aura) > 0);
                    }
                }
                stacks.apply(&r.event);
            }
        }
        assert!(full >= 4, "{full} full lifetimes");
    }
}

#[test]
fn offering_from_beyond_takes_3_seconds_off_stormkeeper_per_call() {
    let f = farseer();
    let mut checked = 0;
    for seed in 0..10 {
        let trace = f.rollout(seed);
        let calls = completed(&trace, CALL_OF_THE_ANCESTORS);
        let owner_starts: Vec<(SimTime, SpellId)> = trace
            .iter()
            .filter_map(|r| match r.event {
                TraceEvent::CastStart { actor, spell, .. } if actor == OWNER => {
                    Some((r.time, spell))
                }
                _ => None,
            })
            .collect();
        for end in completed(&trace, STORMKEEPER) {
            let mut ready = end + SimDuration(60_000);
            for &c in calls.iter().filter(|&&c| c >= end) {
                if c < ready {
                    ready = SimTime(ready.0.saturating_sub(3_000).max(c.0));
                }
            }
            // Stormkeeper tops the priority: the seat's first cast once it
            // is ready is Stormkeeper, within one cast or GCD.
            let Some(&(next, spell)) = owner_starts.iter().find(|&&(t, _)| t >= ready) else {
                continue;
            };
            assert_eq!(
                spell, STORMKEEPER,
                "{spell:?} at {next:?}, ready at {ready:?}"
            );
            assert!(next.saturating_since(ready) <= SimDuration(2_500));
            let previous = owner_starts.iter().rev().find(|&&(t, _)| t < ready);
            assert!(previous.is_some_and(|&(_, s)| s != STORMKEEPER));
            checked += 1;
        }
    }
    assert!(checked >= 8, "{checked} cooldowns checked");
}

#[test]
fn lava_burst_and_maelstrom_follow_the_talents() {
    let f = farseer();
    let mut k = f.kernel(2);
    assert!(play_until(&mut k, |_| true).is_none());
    let m = f.mechanics(2);
    let state = k.state();
    let haste = m.math().haste_mult(state, OWNER);
    let cast = (2000.0 / haste * 0.9).round() as u32;
    assert_eq!(state.cast_time(Seat(0), LAVA_BURST), SimDuration(cast));
    assert_eq!(state.cooldown(OWNER, LAVA_BURST).unwrap().max_charges, 2);
    let maelstrom = state.resource(OWNER, ResourceKind::Maelstrom).unwrap();
    assert_eq!(maelstrom.max, 125.0);

    // Ancestral Swiftness makes the next Lava Burst instant and is spent by
    // it; the GCD is Windspeaker's.
    let swift = |k: &FarseerKernel| {
        holds(k, ANCESTRAL_SWIFTNESS_BUFF) > 0
            && k.legal(Seat(0)).abilities.get(&LAVA_BURST) == Some(&Readiness::Now)
    };
    assert!(play_until(&mut k, swift).is_none());
    let now = k.state().now();
    k.submit(
        Seat(0),
        Choice::Cast {
            ability: LAVA_BURST,
            target: TargetSel::Primary,
            opts: CastOpts::default(),
        },
    )
    .unwrap();
    assert_eq!(holds(&k, ANCESTRAL_SWIFTNESS_BUFF), 0);
    let gcd = ((1500.0 / haste * 0.9).round() as u32).max(750);
    assert_eq!(k.state().gcd_end(Seat(0)), Some(now + SimDuration(gcd)));
    let trace = k.drain_trace();
    let last = trace
        .iter()
        .rev()
        .find(|r| completed_by(r, OWNER, &[LAVA_BURST]));
    assert_eq!(last.map(|r| r.time), Some(now), "instant");
}

#[test]
fn the_checker_rejects_bad_shared_decks_and_empty_choices() {
    let mut f = farseer();
    assert_eq!(check_game_data(&f.data), vec![]);
    let routine = f.data.auras.get_mut(&ROUTINE_COMMUNICATION_AURA).unwrap();
    routine.listeners[1].chance = ProcChance::Deck {
        successes: 1,
        size: 50,
    };
    routine.listeners[2].shared_with = Some(2);
    let issues = check_game_data(&f.data);
    let bad = DataIssue::InvalidSharedProc(Owner::Aura(ROUTINE_COMMUNICATION_AURA));
    assert_eq!(
        issues.iter().filter(|&i| *i == bad).count(),
        2,
        "{issues:?}"
    );

    let mut f = farseer();
    let calling = f.data.auras.get_mut(&AuraId(443446)).unwrap();
    calling.listeners[0].effects = vec![Effect::ApplyOneOf {
        auras: Vec::new(),
        target: EffectTarget::Caster,
    }];
    assert!(check_game_data(&f.data).contains(&DataIssue::EmptyChoice(Owner::Aura(AuraId(443446)))));
}
