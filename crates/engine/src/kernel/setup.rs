//! Checking a [`RunSetup`] and building the starting [`World`].

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use portunus_core::rng::{self, Purpose};
use portunus_core::{ActorId, Sample, Seat, SimDuration, SimTime, Trigger, PARTY_SIZE};
use portunus_gamedata::effect::{ModKind, Modifier};
use portunus_gamedata::enemy::{EnemyAction, EnemySubject};
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::stats::ResourceDef;
use portunus_scenario::resolved::{Segment, SpawnIndex, SpawnSet};

use crate::error::{EngineError, SetupIssue};
use crate::mechanics::whole_points;
use crate::outcome::SeatOutcome;
use crate::setup::RunSetup;
use crate::state::{ActorKind, SeatPhase};

use super::queue::Queue;
use super::world::{Actor, Resource, SeatState, Seg, Statics, Trace, World};

fn issues(setup: &RunSetup) -> Vec<SetupIssue> {
    let data = &setup.data;
    let mut out = Vec::new();
    if setup.seats.is_empty() {
        out.push(SetupIssue::NoSeats);
    }
    if setup.seats.len() > PARTY_SIZE {
        out.push(SetupIssue::TooManySeats(setup.seats.len()));
    }
    if data.build != setup.enemies.build {
        out.push(SetupIssue::BuildMismatch {
            game: data.build.clone(),
            enemies: setup.enemies.build.clone(),
        });
    }
    for seat in &setup.seats {
        let t = &seat.template;
        for &s in &t.abilities {
            if !data.spells.contains_key(&s) {
                out.push(SetupIssue::UnknownSpell(s));
            }
        }
        for &a in &t.passive_auras {
            if !data.auras.contains_key(&a) {
                out.push(SetupIssue::UnknownAura(a));
            }
        }
        if let Some(p) = t.permanent_pet {
            if !data.pets.contains_key(&p) {
                out.push(SetupIssue::UnknownPet(p));
            }
        }
    }
    let externals = &setup.externals;
    for &a in externals.party_auras.iter().chain(&externals.enemy_auras) {
        if !data.auras.contains_key(&a) {
            out.push(SetupIssue::UnknownAura(a));
        }
    }
    for seg in &setup.run.segments {
        if let Segment::Combat(c) = seg {
            for spawn in &c.spawns {
                if !setup.enemies.enemies.contains_key(&spawn.enemy) {
                    out.push(SetupIssue::UnknownEnemy(spawn.enemy.clone()));
                }
            }
        }
    }
    out
}

fn spawns_adds(a: &EnemyAction) -> bool {
    match a {
        EnemyAction::SpawnAdds { .. } => true,
        EnemyAction::Cast { then, .. } => spawns_adds(then),
        EnemyAction::Sequence(actions) => actions.iter().any(spawns_adds),
        _ => false,
    }
}

/// Per combat, spawn, and rule: each rule's trigger with its subjects
/// resolved to the spawn it belongs to.
fn rule_triggers(setup: &RunSetup) -> Vec<Vec<Vec<Trigger<SpawnSet>>>> {
    setup
        .run
        .segments
        .iter()
        .filter_map(|s| match s {
            Segment::Combat(c) => Some(c),
            Segment::Travel { .. } => None,
        })
        .map(|c| {
            c.spawns
                .iter()
                .enumerate()
                .map(|(i, spawn)| {
                    let me = SpawnIndex(i as u16);
                    setup
                        .enemies
                        .enemies
                        .get(&spawn.enemy)
                        .map(|def| {
                            def.rules
                                .iter()
                                .map(|r| {
                                    r.when.map(&mut |subject| match subject {
                                        EnemySubject::Itself => SpawnSet::One(me),
                                        EnemySubject::Combat => SpawnSet::Engaged,
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect()
}

fn unsupported(setup: &RunSetup) -> Option<&'static str> {
    let data = &setup.data;
    for seat in &setup.seats {
        let t = &seat.template;
        if t.main_hand.is_some() || t.off_hand.is_some() {
            return Some("auto-attacks");
        }
        if seat.latency.cast_lag.bounds() != (SimDuration::ZERO, SimDuration::ZERO) {
            return Some("cast lag");
        }
        for s in t.abilities.iter().filter_map(|s| data.spells.get(s)) {
            if matches!(s.cast, CastKind::Channel { .. } | CastKind::Empower { .. }) {
                return Some("channels and empowers");
            }
            if s.cooldown.is_some_and(|c| c.category.is_some()) {
                return Some("shared cooldown categories");
            }
        }
    }
    for seg in &setup.run.segments {
        let Segment::Combat(c) = seg else { continue };
        for spawn in &c.spawns {
            if setup
                .enemies
                .enemies
                .get(&spawn.enemy)
                .is_some_and(|e| e.rules.iter().any(|r| spawns_adds(&r.action)))
            {
                return Some("enemy adds");
            }
        }
    }
    None
}

pub(crate) fn validate(setup: &RunSetup) -> Result<(), EngineError> {
    let found = issues(setup);
    if !found.is_empty() {
        return Err(EngineError::Setup(found));
    }
    match unsupported(setup) {
        Some(what) => Err(EngineError::Unsupported(what.to_owned())),
        None => Ok(()),
    }
}

/// The world at time zero: seats created, nothing applied or scheduled.
pub(crate) fn build(setup: RunSetup) -> World {
    let n = setup.seats.len();
    let names: Vec<String> = (0..n).map(|i| format!("seat{i}")).collect();
    let latency = names
        .iter()
        .map(|name| {
            [
                rng::domain(Purpose::Reaction, &[name.as_str(), "anticipated"]),
                rng::domain(Purpose::Reaction, &[name.as_str(), "reaction"]),
            ]
        })
        .collect();
    let gcd = setup
        .seats
        .iter()
        .map(|s| {
            s.template
                .abilities
                .iter()
                .filter_map(|s| setup.data.spells.get(s)?.gcd)
                .max_by_key(|g| g.base)
        })
        .collect();
    let abilities: Vec<Vec<_>> = setup
        .seats
        .iter()
        .map(|s| s.template.abilities.iter().copied().collect())
        .collect();
    let actors = setup
        .seats
        .iter()
        .zip(&names)
        .enumerate()
        .map(|(i, (seat, name))| {
            let t = &seat.template;
            let mut actor = Actor::new(
                ActorKind::Player(Seat(i as u8)),
                Arc::from(name.as_str()),
                whole_points(t.derived.max_health),
                1.0 + t.derived.haste_pct / 100.0,
            );
            let passives: Vec<&Modifier> = t
                .passive_auras
                .iter()
                .filter_map(|a| setup.data.auras.get(a))
                .flat_map(|a| &a.modifiers)
                .collect();
            actor.resources = t
                .resources
                .iter()
                .map(|&def| {
                    let extra: f64 = passives
                        .iter()
                        .filter(|m| m.kind == ModKind::ResourceMax(def.kind))
                        .map(|m| m.value)
                        .sum();
                    let def = ResourceDef {
                        max: (def.max + extra).max(0.0),
                        ..def
                    };
                    Resource {
                        def,
                        value: def.initial.min(def.max),
                        at: SimTime::ZERO,
                        regen_mult: 1.0,
                    }
                })
                .collect();
            actor
        })
        .collect();
    let seats = (0..n)
        .map(|i| SeatState {
            phase: SeatPhase::Idle,
            wait: None,
            gen: 0,
            armed: None,
            gcd_end: None,
            last_cast: None,
            unperceived: Vec::new(),
            perception_ids: Vec::new(),
            waited: None,
            draws: [0, 0],
            summons: 0,
            outcome: SeatOutcome {
                seat: Seat(i as u8),
                damage_done: 0,
                casts: 0,
                deaths: 0,
                damage_taken: 0,
                healing_done: 0,
                demands_failed: 0,
            },
            moving: None,
            move_gen: 0,
            demands: Vec::new(),
        })
        .collect();
    let record = setup.record_trace;
    let roles = setup.seats.iter().map(|s| s.template.role).collect();
    let rule_triggers = rule_triggers(&setup);
    World {
        s: Arc::new(Statics {
            setup,
            latency,
            abilities,
            gcd,
            roles,
            rule_triggers,
        }),
        now: SimTime::ZERO,
        seg: Seg::Finished,
        actors,
        seat_actors: (0..n).map(|i| ActorId(i as u16)).collect(),
        seats,
        pets: vec![Vec::new(); n],
        enemies: Vec::new(),
        projectiles: Vec::new(),
        flights: Vec::new(),
        stashed: Vec::new(),
        departing: Vec::new(),
        timers: Vec::new(),
        timer_ids: Vec::new(),
        streams: BTreeMap::new(),
        decks: BTreeMap::new(),
        queue: Queue::default(),
        followups: VecDeque::new(),
        batch: None,
        pulls: Vec::new(),
        next_id: 0,
        triggers_queued: None,
        delayed: BTreeMap::new(),
        fault: None,
        trace: Trace::new(record),
        finished: None,
    }
}
