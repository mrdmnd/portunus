//! Checking a [`RunSetup`] and building the starting [`World`].

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use portunus_core::rng::{self, Purpose};
use portunus_core::{ActorId, EnemyKey, Seat, SimTime, SpellId, PARTY_SIZE};
use portunus_gamedata::effect::{ModKind, Modifier};
use portunus_gamedata::enemy::EnemyAction;
use portunus_gamedata::spell::Requirement;
use portunus_gamedata::stats::ResourceDef;
use portunus_scenario::resolved::Segment;

use crate::error::{EngineError, SetupIssue};
use crate::mechanics::whole_points;
use crate::outcome::SeatOutcome;
use crate::setup::{Externals, RunSetup};
use crate::state::{ActorKind, SeatPhase, RECENT_CASTS};

use super::queue::Queue;
use super::world::{Actor, Resource, SeatState, Seg, Statics, Swing, Trace, World};

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
    let pulled = externals
        .on_pull
        .iter()
        .flat_map(|p| std::iter::once(p.aura).chain(p.lockout));
    for a in externals
        .party_auras
        .iter()
        .chain(&externals.enemy_auras)
        .copied()
        .chain(pulled)
    {
        if !data.auras.contains_key(&a) {
            out.push(SetupIssue::UnknownAura(a));
        }
    }
    let mut pending: Vec<&EnemyKey> = setup
        .run
        .segments
        .iter()
        .filter_map(|seg| match seg {
            Segment::Combat(c) => Some(c),
            Segment::Travel { .. } => None,
        })
        .flat_map(|c| c.spawns.iter().map(|s| &s.enemy))
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(key) = pending.pop() {
        if !seen.insert(key) {
            continue;
        }
        let Some(def) = setup.enemies.enemies.get(key) else {
            out.push(SetupIssue::UnknownEnemy(key.clone()));
            continue;
        };
        for rule in &def.rules {
            spawned(&rule.action, &mut pending);
        }
    }
    out
}

/// The enemies `action` can spawn.
fn spawned<'a>(action: &'a EnemyAction, out: &mut Vec<&'a EnemyKey>) {
    match action {
        EnemyAction::SpawnAdds { adds, .. } => out.extend(adds.iter().map(|(key, _)| key)),
        EnemyAction::Cast { then, .. } => spawned(then, out),
        EnemyAction::Sequence(actions) => actions.iter().for_each(|a| spawned(a, out)),
        _ => {}
    }
}

/// The externals plus what each class among the seats brings, in that
/// order, each aura once.
fn group(setup: &RunSetup) -> Externals {
    let data = &setup.data;
    let classes = setup
        .seats
        .iter()
        .filter_map(|s| data.specs.get(&s.template.spec))
        .filter_map(|spec| data.classes.get(&spec.class));
    let mut group = setup.externals.clone();
    for class in classes {
        group.party_auras.extend(&class.group_auras);
        group.enemy_auras.extend(&class.enemy_auras);
        group.on_pull.extend(&class.on_pull);
    }
    let mut seen = BTreeSet::new();
    group.party_auras.retain(|a| seen.insert(*a));
    let mut seen = BTreeSet::new();
    group.enemy_auras.retain(|a| seen.insert(*a));
    let mut seen = BTreeSet::new();
    group.on_pull.retain(|p| seen.insert(p.aura));
    group
}

/// The `TargetHpAtMost` fractions among `abilities` and the spells auras
/// can turn them into, ascending and deduplicated.
fn hp_thresholds(setup: &RunSetup, abilities: &[SpellId]) -> Vec<f64> {
    let data = &setup.data;
    let overridden = data
        .auras
        .values()
        .flat_map(|a| &a.overrides)
        .filter(|(from, _)| abilities.contains(from))
        .map(|&(_, to)| to);
    let mut found: Vec<f64> = abilities
        .iter()
        .copied()
        .chain(overridden)
        .filter_map(|s| data.spells.get(&s))
        .flat_map(|d| &d.requires)
        .filter_map(|r| match *r {
            Requirement::TargetHpAtMost(f) => Some(f),
            _ => None,
        })
        .collect();
    found.sort_by(f64::total_cmp);
    found.dedup();
    found
}

pub(crate) fn validate(setup: &RunSetup) -> Result<(), EngineError> {
    let found = issues(setup);
    if found.is_empty() {
        Ok(())
    } else {
        Err(EngineError::Setup(found))
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
                rng::domain(Purpose::Reaction, &[name.as_str(), "cast_lag"]),
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
            actor.swings = [t.main_hand.map(Swing::new), t.off_hand.map(Swing::new)];
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
                    Resource::new(def, SimTime::ZERO, false)
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
            recent_casts: [None; RECENT_CASTS],
            unperceived: Vec::new(),
            perception_ids: Vec::new(),
            waited: None,
            draws: [0, 0, 0],
            lagged: None,
            summons: 0,
            outcome: SeatOutcome {
                seat: Seat(i as u8),
                damage_done: 0,
                damage_absorbed: 0,
                casts: 0,
                deaths: 0,
                damage_taken: 0,
                healing_done: 0,
                demands_failed: 0,
            },
            moving: None,
            move_gen: 0,
            moving_told: false,
            demands: Vec::new(),
        })
        .collect();
    let record = setup.record_trace;
    let roles = setup.seats.iter().map(|s| s.template.role).collect();
    let hp_thresholds = abilities
        .iter()
        .map(|list| hp_thresholds(&setup, list))
        .collect();
    let group = group(&setup);
    World {
        s: Arc::new(Statics {
            setup,
            group,
            latency,
            abilities,
            gcd,
            roles,
            hp_thresholds,
        }),
        now: SimTime::ZERO,
        seg: Seg::Finished,
        actors,
        seat_actors: (0..n).map(|i| ActorId(i as u16)).collect(),
        seats,
        pets: vec![Vec::new(); n],
        enemies: Vec::new(),
        spawns: Vec::new(),
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
