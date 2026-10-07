//! Pets, guardians, totems, and deck-of-cards procs, on the checked-in
//! Elemental data plus hand-written placeholder definitions.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, PetId, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::mechanics::whole_points;
use portunus_engine::trace::{CastEndReason, TraceEvent};
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, Outcome, Readiness, RunSetup, SeatSetup,
    StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::aura::{AuraDef, Periodic, RefreshRule};
use portunus_gamedata::effect::{
    Coefficient, Effect, EffectTarget, ListenFor, Listener, ModKind, ModScope, Modifier, ProcChance,
};
use portunus_gamedata::item::{WeaponDef, WeaponHand};
use portunus_gamedata::pet::{PetDef, PetKind, PetScaling};
use portunus_gamedata::spell::{CastKind, CooldownDef, GcdDef, SpellDef, Targeting};
use portunus_gamedata::stats::{Cost, ResourceDef, ResourceKind, SchoolMask, SpendScaling};
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{
    check_game_data, read_ron, DataIssue, EnemyDataSource, GameDataSource, Owner, RonFile,
};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{CombatMath, EffectCtx, Kits, Outgoing, PartyMechanics};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

const LIGHTNING_BOLT: SpellId = SpellId(188196);
const LAVA_BURST: SpellId = SpellId(51505);
const FLAME_SHOCK: SpellId = SpellId(188389);
const EARTH_SHOCK: SpellId = SpellId(8042);

const FIRE_ELEMENTAL: SpellId = SpellId(198067);
const FIRE_ELEMENTAL_PET: PetId = PetId(95061);
const FIRE_BLAST: SpellId = SpellId(57984);

const LIQUID_MAGMA_TOTEM: SpellId = SpellId(192222);
const LIQUID_MAGMA_TOTEM_PET: PetId = PetId(97369);
const MAGMA_PULSE: AuraId = AuraId(192226);

const PRIMAL_FIRE_ELEMENTAL: PetId = PetId(61029);
const METEOR_COMMAND: SpellId = SpellId(117588);
const METEOR: SpellId = SpellId(117589);
const RESUMMON: SpellId = SpellId(900_001);

const DECK_AURA: AuraId = AuraId(900_002);
const DECK_MARKER: AuraId = AuraId(900_003);
const TOGGLE: SpellId = SpellId(900_004);

const HUNTER_PET: PetId = PetId(165_189);
const BITE: SpellId = SpellId(17_253);
const CALL_PET: SpellId = SpellId(883);
const KILL_COMMAND: SpellId = SpellId(34_026);
const KILL_COMMAND_HIT: SpellId = SpellId(83_381);
const FRENZY: AuraId = AuraId(900_005);
const SWING_MARKER: AuraId = AuraId(900_006);
const BLOODLUST: SpellId = SpellId(2825);
const BLOODLUST_AURA: AuraId = AuraId(2825);
const OWNER_BUFFS: AuraId = AuraId(900_007);

const SPELL_POWER: f64 = 2600.0;

fn ms(n: u32) -> SimDuration {
    SimDuration(n)
}

fn gcd() -> Option<GcdDef> {
    Some(GcdDef {
        base: ms(1500),
        hasted: true,
        floor: ms(750),
    })
}

fn cooldown(millis: u32) -> Option<CooldownDef> {
    Some(CooldownDef {
        duration: ms(millis),
        charges: 1,
        hasted: false,
        category: None,
    })
}

fn spell(id: SpellId, cast: CastKind, gcd: Option<GcdDef>, effects: Vec<Effect>) -> SpellDef {
    SpellDef {
        id,
        name: format!("test spell {}", id.0),
        school: SchoolMask::FIRE,
        cast,
        gcd,
        cooldown: None,
        costs: Vec::new(),
        targeting: Targeting::Enemy,
        hostile: true,
        range: None,
        speed: None,
        min_travel: SimDuration::ZERO,
        rolls_on_impact: false,
        castable_while_moving: false,
        usable_while_casting: false,
        effects,
    }
}

fn fire_damage(sp: f64, target: EffectTarget) -> Effect {
    Effect::Damage {
        amount: Coefficient::SpellPower(sp),
        school: SchoolMask::FIRE,
        target,
        aoe: None,
    }
}

fn aura(id: AuraId, duration: Option<SimDuration>) -> AuraDef {
    AuraDef {
        id,
        name: format!("test aura {}", id.0),
        duration,
        max_stacks: 1,
        refresh: RefreshRule::Replace,
        periodic: None,
        value: None,
        modifiers: Vec::new(),
        listeners: Vec::new(),
        overrides: Vec::new(),
        on_expire: Vec::new(),
        cancelable: false,
        blocked_by: None,
    }
}

fn pet(id: PetId, kind: PetKind, sp_from_sp: f64) -> PetDef {
    PetDef {
        id,
        name: format!("pet{}", id.0),
        kind,
        scaling: PetScaling {
            sp_from_sp,
            health: 0.5,
            ..PetScaling::default()
        },
        melee: None,
        autocast: Vec::new(),
        passive_auras: Vec::new(),
        resources: Vec::new(),
        duration: None,
        max_active: None,
    }
}

struct Fixture {
    data: GameData,
    enemies: Arc<EnemyData>,
    template: ActorTemplate,
    sampler: Sampler,
}

fn fixture() -> Fixture {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    let mut enemies: EnemyData =
        EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    // The fight lengths below assume a 400k dummy.
    for e in enemies.enemies.values_mut() {
        e.health = 400_000;
    }
    let loadout: Loadout = read_ron(&dir.join("loadouts/elemental.ron")).unwrap();
    let template = Compiler.compile(&data, &loadout).unwrap();
    let spec: ScenarioSpec = read_ron(&dir.join("scenarios/target_dummy.ron")).unwrap();
    let enemies = Arc::new(enemies);
    let sampler = Sampler::new(spec, Arc::clone(&enemies)).unwrap();
    Fixture {
        data,
        enemies,
        template,
        sampler,
    }
}

impl Fixture {
    fn add_spell(&mut self, s: SpellDef) {
        self.data.spells.insert(s.id, s);
    }

    fn add_ability(&mut self, s: SpellDef) {
        self.template.abilities.insert(s.id);
        self.add_spell(s);
    }

    fn kernel(&self, seed: u64) -> Kernel<PartyMechanics> {
        let setup = self.setup(seed);
        let mechanics = PartyMechanics::new(&setup, Arc::new(Kits::standard())).unwrap();
        Kernel::new(setup, mechanics).unwrap()
    }

    fn setup(&self, seed: u64) -> RunSetup {
        RunSetup {
            run: Arc::new(self.sampler.sample(Seed(seed))),
            data: Arc::new(self.data.clone()),
            enemies: Arc::clone(&self.enemies),
            seats: vec![SeatSetup {
                template: self.template.clone(),
                latency: Latency {
                    anticipated: Dist::Fixed(SimDuration::ZERO),
                    reaction: Dist::Fixed(SimDuration::ZERO),
                    cast_lag: Dist::Fixed(SimDuration::ZERO),
                },
            }],
            externals: Externals::default(),
            record_trace: true,
        }
    }

    /// Presses `first` whenever ready, then a basic Elemental priority.
    fn rollout(&self, seed: u64, first: &[SpellId]) -> (Outcome, Vec<TraceRecord>) {
        self.rollout_with(seed, |k, seat| priority(k, seat, first))
    }

    fn rollout_with(
        &self,
        seed: u64,
        policy: impl Fn(&Kernel<PartyMechanics>, Seat) -> Choice,
    ) -> (Outcome, Vec<TraceRecord>) {
        let mut k = self.kernel(seed);
        for _ in 0..100_000 {
            match k.advance().unwrap() {
                Step::Done(o) => return (o, k.drain_trace()),
                Step::Decide(req) => {
                    let choice = policy(&k, req.seat);
                    k.submit(req.seat, choice).unwrap();
                }
            }
        }
        panic!("the run should end");
    }

    fn mechanics(&self, seed: u64) -> PartyMechanics {
        PartyMechanics::new(&self.setup(seed), Arc::new(Kits::standard())).unwrap()
    }
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

fn completed(trace: &[TraceRecord], want: SpellId) -> Vec<(SimTime, ActorId)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::CastEnd {
                actor,
                spell,
                reason: CastEndReason::Completed,
                ..
            } if spell == want => Some((r.time, actor)),
            _ => None,
        })
        .collect()
}

fn priority(k: &Kernel<PartyMechanics>, seat: Seat, first: &[SpellId]) -> Choice {
    let mask = k.legal(seat);
    let ready = |s: SpellId| mask.abilities.get(&s) == Some(&Readiness::Now);
    let state = k.state();
    let me = state.seats()[usize::from(seat.0)];
    let dot_up = state
        .target(me)
        .is_some_and(|t| state.auras(t).iter().any(|a| a.aura.0 == FLAME_SHOCK.0));
    let pick = first
        .iter()
        .copied()
        .find(|&s| ready(s))
        .or_else(|| (!dot_up && ready(FLAME_SHOCK)).then_some(FLAME_SHOCK))
        .or_else(|| {
            [LAVA_BURST, EARTH_SHOCK, LIGHTNING_BOLT]
                .into_iter()
                .find(|&s| ready(s))
        });
    match pick {
        Some(ability) => Choice::Cast {
            ability,
            target: TargetSel::Primary,
            opts: CastOpts::default(),
        },
        None => Choice::Wait(Wait::NextEvent),
    }
}

/// When each pet of `kind` arrived and, if it did, left.
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

fn damage_by(trace: &[TraceRecord], source: ActorId) -> Vec<(SimTime, Option<SpellId>, bool, u64)> {
    trace
        .iter()
        .filter_map(|r| match &r.event {
            TraceEvent::Damage(d) if d.source == source => {
                Some((r.time, d.spell, d.crit, d.amount))
            }
            _ => None,
        })
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn with_fire_elemental(f: &mut Fixture) {
    let mut fe = pet(FIRE_ELEMENTAL_PET, PetKind::Guardian, 0.5);
    fe.autocast = vec![FIRE_BLAST];
    fe.duration = Some(ms(30_000));
    f.data.pets.insert(fe.id, fe);
    add_fire_blast(f);
    let mut summon = spell(
        FIRE_ELEMENTAL,
        CastKind::Instant,
        gcd(),
        vec![Effect::Summon {
            pet: FIRE_ELEMENTAL_PET,
            count: 1,
            duration: None,
        }],
    );
    summon.cooldown = cooldown(150_000);
    f.add_ability(summon);
}

#[test]
fn guardians_cast_on_their_own_and_expire() {
    let mut f = fixture();
    with_fire_elemental(&mut f);
    let (outcome, trace) = f.rollout(1, &[FIRE_ELEMENTAL]);
    assert!(outcome.completed);

    let k = f.kernel(1);
    let mechanics = PartyMechanics::new(&f.setup(1), Arc::new(Kits::standard())).unwrap();
    let owner = mechanics.math().derived(k.state(), k.state().seats()[0]);
    let normal = 0.5 * SPELL_POWER * (1.0 + owner.versatility_pct / 100.0);

    let lives = lifetimes(&trace, FIRE_ELEMENTAL_PET);
    assert_eq!(lives.len(), 2, "a 150 s cooldown in a ~180 s fight");
    let first = lives.values().next().unwrap();
    assert_eq!(first.1, Some(first.0 + ms(30_000)), "guardians time out");
    for (&actor, &(from, until)) in &lives {
        let full = until.is_some();
        let until = until.unwrap_or(outcome.end_time);
        let hits = damage_by(&trace, actor);
        assert!(!full || hits.len() >= 10, "{} Fire Blasts", hits.len());
        for &(at, spell, crit, amount) in &hits {
            assert!(at > from && at <= until);
            assert_eq!(spell, Some(FIRE_BLAST));
            // Pets have the owner's crit chance but not Elemental Fury.
            let want = if crit { normal * 2.0 } else { normal };
            assert_eq!(amount, whole_points(want), "{want}");
        }
    }
    let starts = trace
        .iter()
        .filter(|r| matches!(r.event, TraceEvent::CastStart { spell, .. } if spell == FIRE_BLAST))
        .count();
    let summons = trace
        .iter()
        .filter(|r| {
            matches!(r.event, TraceEvent::CastEnd { spell, reason: CastEndReason::Completed, .. }
                if spell == FIRE_ELEMENTAL)
        })
        .count();
    assert_eq!(summons, 2);
    assert!(starts >= 10);
}

#[test]
fn forks_with_live_guardians_agree() {
    let mut f = fixture();
    with_fire_elemental(&mut f);
    let finish = |k: &mut Kernel<PartyMechanics>| loop {
        match k.advance().unwrap() {
            Step::Done(o) => return (o.end_time, k.trace_hash()),
            Step::Decide(req) => {
                let choice = priority(k, req.seat, &[FIRE_ELEMENTAL]);
                k.submit(req.seat, choice).unwrap();
            }
        }
    };
    let mut k = f.kernel(4);
    loop {
        let Step::Decide(req) = k.advance().unwrap() else {
            panic!("a guardian should be summoned");
        };
        if !k.state().pets(Seat(0)).is_empty() {
            break;
        }
        let choice = priority(&k, req.seat, &[FIRE_ELEMENTAL]);
        k.submit(req.seat, choice).unwrap();
    }
    let mut fork = k.clone();
    let req = match k.advance().unwrap() {
        Step::Decide(req) => req,
        Step::Done(_) => panic!("mid-fight"),
    };
    let choice = priority(&k, req.seat, &[FIRE_ELEMENTAL]);
    k.submit(req.seat, choice.clone()).unwrap();
    let Step::Decide(_) = fork.advance().unwrap() else {
        panic!("mid-fight");
    };
    fork.submit(req.seat, choice).unwrap();
    assert_eq!(finish(&mut k), finish(&mut fork));
}

#[test]
fn totems_pulse_and_replace_each_other() {
    let mut f = fixture();
    with_magma_totem(&mut f);
    let (_, trace) = f.rollout(2, &[LIQUID_MAGMA_TOTEM]);
    let lives = lifetimes(&trace, LIQUID_MAGMA_TOTEM_PET);
    assert!(lives.len() > 10);
    let mut replaced = 0;
    let ordered: Vec<_> = lives.values().copied().collect();
    for pair in ordered.windows(2) {
        let (Some(gone), next) = (pair[0].1, pair[1].0) else {
            panic!("only the last totem may outlive the fight");
        };
        assert!(gone <= next, "one totem at a time");
        if gone == next {
            replaced += 1;
        }
    }
    assert!(replaced > 0, "a 10 s cooldown replaces a 12 s totem");
    for (&actor, &(from, until)) in &lives {
        let Some(until) = until else { continue };
        let pulses = damage_by(&trace, actor);
        assert!(pulses.iter().all(|p| p.0 > from && p.0 <= until));
        let whole = ((until - from).millis() / 2000) as usize;
        assert!(
            pulses.len() == whole || pulses.len() + 1 == whole,
            "{} pulses in {} ms",
            pulses.len(),
            (until - from).millis()
        );
    }
}

fn with_magma_totem(f: &mut Fixture) {
    let mut totem = pet(LIQUID_MAGMA_TOTEM_PET, PetKind::Totem, 0.0);
    totem.passive_auras = vec![MAGMA_PULSE];
    totem.duration = Some(ms(12_000));
    totem.max_active = Some(1);
    f.data.pets.insert(totem.id, totem);
    let mut pulse = aura(MAGMA_PULSE, None);
    pulse.periodic = Some(Periodic {
        period: ms(2000),
        hasted: false,
        partial_final_tick: false,
        effects: vec![fire_damage(0.1, EffectTarget::AllEnemies)],
    });
    f.data.auras.insert(pulse.id, pulse);
    let mut drop = spell(
        LIQUID_MAGMA_TOTEM,
        CastKind::Instant,
        gcd(),
        vec![Effect::Summon {
            pet: LIQUID_MAGMA_TOTEM_PET,
            count: 1,
            duration: None,
        }],
    );
    drop.cooldown = cooldown(10_000);
    f.add_ability(drop);
}

#[test]
fn permanent_pets_persist_and_take_commands() {
    let mut f = fixture();
    let mut primal = pet(PRIMAL_FIRE_ELEMENTAL, PetKind::Pet, 1.0);
    primal.autocast = vec![FIRE_BLAST];
    f.data.pets.insert(primal.id, primal);
    add_fire_blast(&mut f);
    f.add_spell(spell(
        METEOR,
        CastKind::Instant,
        None,
        vec![fire_damage(3.0, EffectTarget::Target)],
    ));
    let mut command = spell(
        METEOR_COMMAND,
        CastKind::Instant,
        gcd(),
        vec![Effect::CommandPet {
            pet: Some(PRIMAL_FIRE_ELEMENTAL),
            spell: METEOR,
        }],
    );
    command.cooldown = cooldown(20_000);
    f.add_ability(command);
    let mut resummon = spell(
        RESUMMON,
        CastKind::Instant,
        gcd(),
        vec![Effect::Summon {
            pet: PRIMAL_FIRE_ELEMENTAL,
            count: 3,
            duration: Some(ms(1000)),
        }],
    );
    resummon.cooldown = cooldown(50_000);
    f.add_ability(resummon);
    f.template.permanent_pet = Some(PRIMAL_FIRE_ELEMENTAL);

    let k = f.kernel(3);
    let state = k.state();
    let first = state.pets(Seat(0)).to_vec();
    assert_eq!(first.len(), 1, "summoned at run start");
    let view = state.actor(first[0]).unwrap();
    assert_eq!(view.expires, None);
    assert_eq!(state.target(first[0]), state.target(state.seats()[0]));

    let (_, trace) = f.rollout(3, &[METEOR_COMMAND, RESUMMON]);
    let completed = |want: SpellId| {
        trace
            .iter()
            .filter(|r| {
                matches!(r.event, TraceEvent::CastEnd { spell, reason: CastEndReason::Completed, .. }
                    if spell == want)
            })
            .count()
    };
    let lives = lifetimes(&trace, PRIMAL_FIRE_ELEMENTAL);
    let resummons = completed(RESUMMON);
    assert!(resummons >= 2);
    assert_eq!(
        lives.len(),
        1 + resummons,
        "one at start, then one per resummon whatever the count"
    );
    let ordered: Vec<_> = lives.values().copied().collect();
    for pair in ordered.windows(2) {
        assert_eq!(pair[0].1, Some(pair[1].0), "a new pet replaces the old");
    }
    assert!(ordered.last().unwrap().1.is_none(), "pets don't time out");

    let commands = completed(METEOR_COMMAND);
    let meteors: usize = lives
        .keys()
        .map(|&p| {
            damage_by(&trace, p)
                .iter()
                .filter(|h| h.1 == Some(METEOR))
                .count()
        })
        .sum();
    assert!(commands >= 3);
    assert_eq!(meteors, commands);
    let blasts: usize = lives
        .keys()
        .map(|&p| {
            damage_by(&trace, p)
                .iter()
                .filter(|h| h.1 == Some(FIRE_BLAST))
                .count()
        })
        .sum();
    assert!(blasts > 30, "{blasts} Fire Blasts");
}

#[test]
fn decks_proc_exactly_once_per_deck() {
    let mut f = fixture();
    let mut deck = aura(DECK_AURA, None);
    deck.listeners = vec![Listener {
        on: ListenFor::CastComplete {
            spell: Some(LIGHTNING_BOLT),
            school: None,
        },
        chance: ProcChance::Deck {
            successes: 1,
            size: 4,
        },
        internal_cooldown: None,
        per_unit: false,
        shared_with: None,
        condition: None,
        effects: vec![Effect::ApplyAura {
            aura: DECK_MARKER,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
        }],
    }];
    f.data.auras.insert(deck.id, deck);
    f.data
        .auras
        .insert(DECK_MARKER, aura(DECK_MARKER, Some(ms(500))));
    f.template.passive_auras.push(DECK_AURA);

    let mut positions = Vec::new();
    for seed in 0..5 {
        let (_, trace) = f.rollout(seed, &[]);
        let mut draws: Vec<bool> = Vec::new();
        for r in &trace {
            match r.event {
                TraceEvent::CastEnd {
                    spell,
                    reason: CastEndReason::Completed,
                    ..
                } if spell == LIGHTNING_BOLT => draws.push(false),
                TraceEvent::AuraApplied { aura, .. } if aura == DECK_MARKER => {
                    *draws.last_mut().unwrap() = true;
                }
                _ => {}
            }
        }
        assert!(draws.len() >= 40, "{} bolts", draws.len());
        for chunk in draws.as_chunks::<4>().0 {
            assert_eq!(chunk.iter().filter(|&&d| d).count(), 1, "{chunk:?}");
            positions.push(chunk.iter().position(|&d| d));
        }
    }
    positions.sort();
    positions.dedup();
    assert_eq!(positions.len(), 4, "the hit lands anywhere in the deck");
}

#[test]
fn malformed_decks_are_rejected() {
    let mut f = fixture();
    let mut bad = aura(DECK_AURA, None);
    bad.listeners = vec![Listener {
        on: ListenFor::DamageTaken,
        chance: ProcChance::Deck {
            successes: 5,
            size: 4,
        },
        internal_cooldown: None,
        per_unit: false,
        shared_with: None,
        condition: None,
        effects: Vec::new(),
    }];
    f.data.auras.insert(bad.id, bad);
    assert!(check_game_data(&f.data).contains(&DataIssue::InvalidDeck(Owner::Aura(DECK_AURA))));
}

#[test]
fn decks_persist_across_reapplication() {
    let mut f = fixture();
    let mut deck = aura(DECK_AURA, Some(ms(4000)));
    deck.listeners = vec![Listener {
        on: ListenFor::CastComplete {
            spell: Some(LIGHTNING_BOLT),
            school: None,
        },
        chance: ProcChance::Deck {
            successes: 1,
            size: 4,
        },
        internal_cooldown: None,
        per_unit: false,
        shared_with: None,
        condition: None,
        effects: vec![Effect::ApplyAura {
            aura: DECK_MARKER,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
        }],
    }];
    f.data.auras.insert(deck.id, deck);
    f.data
        .auras
        .insert(DECK_MARKER, aura(DECK_MARKER, Some(ms(500))));
    let mut toggle = spell(
        TOGGLE,
        CastKind::Instant,
        gcd(),
        vec![Effect::ApplyAura {
            aura: DECK_AURA,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
        }],
    );
    toggle.targeting = Targeting::SelfOnly;
    toggle.cooldown = cooldown(4000);
    f.add_ability(toggle);

    for seed in 0..5 {
        let (_, trace) = f.rollout(seed, &[TOGGLE, LIGHTNING_BOLT]);
        let mut draws: Vec<bool> = Vec::new();
        let mut holding = false;
        let mut instances = 0;
        for r in &trace {
            match r.event {
                TraceEvent::AuraApplied { aura, .. } if aura == DECK_AURA => {
                    holding = true;
                    instances += 1;
                }
                TraceEvent::AuraRemoved { aura, .. } if aura == DECK_AURA => holding = false,
                TraceEvent::CastEnd {
                    spell,
                    reason: CastEndReason::Completed,
                    ..
                } if spell == LIGHTNING_BOLT && holding => draws.push(false),
                TraceEvent::AuraApplied { aura, .. } if aura == DECK_MARKER => {
                    *draws.last_mut().unwrap() = true;
                }
                _ => {}
            }
        }
        assert!(instances > 20, "{instances} applications");
        assert!(draws.len() >= 20, "{} draws", draws.len());
        for chunk in draws.as_chunks::<4>().0 {
            assert_eq!(chunk.iter().filter(|&&d| d).count(), 1, "{chunk:?}");
        }
    }
}

fn with_hunter_pet(f: &mut Fixture) {
    let mut p = pet(HUNTER_PET, PetKind::Pet, 0.0);
    p.scaling.ap_from_sp = 1.0;
    p.melee = Some(WeaponDef {
        speed: ms(2000),
        min_damage: 100.0,
        max_damage: 200.0,
        ranged: false,
    });
    p.resources = vec![ResourceDef {
        kind: ResourceKind::Focus,
        max: 100.0,
        initial: 100.0,
        regen_per_sec: 5.0,
        regen_hasted: false,
    }];
    p.autocast = vec![BITE];
    f.data.pets.insert(p.id, p);
    let mut bite = spell(
        BITE,
        CastKind::Instant,
        gcd(),
        vec![Effect::Damage {
            amount: Coefficient::AttackPower(1.0),
            school: SchoolMask::PHYSICAL,
            target: EffectTarget::Target,
            aoe: None,
        }],
    );
    bite.cooldown = cooldown(3000);
    bite.costs = vec![Cost {
        kind: ResourceKind::Focus,
        amount: 50.0,
        extra: 0.0,
        scaling: SpendScaling::None,
    }];
    f.add_spell(bite);
    f.template.permanent_pet = Some(HUNTER_PET);
}

fn add_fire_blast(f: &mut Fixture) {
    f.add_spell(spell(
        FIRE_BLAST,
        CastKind::Cast {
            time: ms(2000),
            hasted: true,
        },
        gcd(),
        vec![fire_damage(1.0, EffectTarget::Target)],
    ));
}

/// Gaps between consecutive times, with the later time.
fn gaps(times: &[SimTime]) -> impl Iterator<Item = (SimTime, SimTime, u32)> + '_ {
    times
        .windows(2)
        .map(|w| (w[0], w[1], (w[1] - w[0]).millis()))
}

fn near(got: u32, want: f64) -> bool {
    (f64::from(got) - want).abs() <= 1.0
}

#[test]
fn pet_melee_and_haste_follow_owner_and_pet_buffs() {
    let mut f = fixture();
    with_hunter_pet(&mut f);
    add_fire_blast(&mut f);
    let mut frenzy = aura(FRENZY, None);
    frenzy.modifiers = vec![
        modifier(ModKind::HastePct, 25.0),
        modifier(ModKind::AttackSpeedPct, 20.0),
    ];
    frenzy.listeners = vec![Listener {
        on: ListenFor::Swing {
            hand: Some(WeaponHand::MainHand),
        },
        chance: ProcChance::Always,
        internal_cooldown: None,
        per_unit: false,
        shared_with: None,
        condition: None,
        effects: vec![Effect::ApplyAura {
            aura: SWING_MARKER,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
        }],
    }];
    f.data.auras.insert(FRENZY, frenzy);
    f.data
        .auras
        .insert(SWING_MARKER, aura(SWING_MARKER, Some(ms(100))));
    let hunter_def = f.data.pets.get_mut(&HUNTER_PET).unwrap();
    hunter_def.passive_auras = vec![FRENZY];
    hunter_def.autocast = vec![FIRE_BLAST];
    let mut lust = aura(BLOODLUST_AURA, Some(ms(40_000)));
    lust.modifiers = vec![modifier(ModKind::HastePct, 30.0)];
    f.data.auras.insert(lust.id, lust);
    let mut bloodlust = spell(
        BLOODLUST,
        CastKind::Instant,
        gcd(),
        vec![Effect::ApplyAura {
            aura: BLOODLUST_AURA,
            target: EffectTarget::Caster,
            stacks: 1,
            duration: None,
        }],
    );
    bloodlust.targeting = Targeting::SelfOnly;
    bloodlust.cooldown = cooldown(600_000);
    f.add_ability(bloodlust);

    let k = f.kernel(5);
    let m = f.mechanics(5);
    let state = k.state();
    let owner = state.seats()[0];
    let hunter = state.pets(Seat(0))[0];
    let haste = m.math().haste_mult(state, owner);
    assert!(close(m.math().haste_mult(state, hunter), haste * 1.25));
    let vers = 1.0 + m.math().derived(state, owner).versatility_pct / 100.0;
    let white = m.math().weapon_damage(state, hunter, WeaponHand::MainHand);
    assert!(close(white, 150.0 + SPELL_POWER / 6.0 * 2.0), "{white}");

    let (_, trace) = f.rollout_with(5, |k, seat| {
        let lust = k.state().now() >= SimTime::ZERO + ms(20_000);
        priority(k, seat, if lust { &[BLOODLUST] } else { &[] })
    });
    let lust_at = completed(&trace, BLOODLUST)[0].0;
    let lust_end = lust_at + ms(40_000);

    let swings: Vec<_> = damage_by(&trace, hunter)
        .into_iter()
        .filter(|h| h.1.is_none())
        .collect();
    assert!(swings.len() > 50, "{} swings", swings.len());
    for &(_, _, crit, amount) in &swings {
        let want = white * vers * if crit { 2.0 } else { 1.0 };
        assert_eq!(amount, whole_points(want), "{want}");
    }
    let markers = trace
        .iter()
        .filter(|r| {
            matches!(r.event, TraceEvent::AuraApplied { holder, aura, .. }
                if holder == hunter && aura == SWING_MARKER)
        })
        .count();
    assert_eq!(markers, swings.len(), "every swing fires Swing listeners");

    let times: Vec<SimTime> = swings.iter().map(|h| h.0).collect();
    let (mut before, mut during) = (0, 0);
    for (from, to, gap) in gaps(&times) {
        if to <= lust_at {
            assert!(near(gap, 2000.0 / (haste * 1.25 * 1.2)), "{gap} ms");
            before += 1;
        } else if from >= lust_at && to <= lust_end {
            assert!(near(gap, 2000.0 / (haste * 1.3 * 1.25 * 1.2)), "{gap} ms");
            during += 1;
        }
    }
    assert!(before > 5 && during > 5, "{before} / {during}");

    let mut started = None;
    let mut casts = 0;
    for r in &trace {
        match r.event {
            TraceEvent::CastStart { actor, spell, .. }
                if actor == hunter && spell == FIRE_BLAST =>
            {
                started = Some(r.time);
            }
            TraceEvent::CastEnd {
                actor,
                reason: CastEndReason::Completed,
                ..
            } if actor == hunter => {
                let start = started.take().unwrap();
                if r.time <= lust_at {
                    // Spell haste is the pet's, without attack speed.
                    assert!(near((r.time - start).millis(), 2000.0 / (haste * 1.25)));
                    casts += 1;
                }
            }
            _ => {}
        }
    }
    assert!(casts > 3, "{casts} Fire Blasts before Bloodlust");
}

#[test]
fn owner_modifiers_follow_the_pet_kind() {
    let mut f = fixture();
    with_fire_elemental(&mut f);
    with_magma_totem(&mut f);
    let mut primal = pet(PRIMAL_FIRE_ELEMENTAL, PetKind::Pet, 1.0);
    primal.autocast = vec![FIRE_BLAST];
    f.data.pets.insert(primal.id, primal);
    f.template.permanent_pet = Some(PRIMAL_FIRE_ELEMENTAL);
    let mut buffs = aura(OWNER_BUFFS, None);
    buffs.modifiers = vec![
        modifier(ModKind::DamageDonePct, 50.0),
        modifier(ModKind::PetDamagePct, 20.0),
        modifier(ModKind::GuardianDamagePct, 10.0),
        modifier(ModKind::CritChanceAdd, 100.0),
    ];
    f.data.auras.insert(buffs.id, buffs);
    f.template.passive_auras.push(OWNER_BUFFS);

    let k = f.kernel(7);
    let m = f.mechanics(7);
    let state = k.state();
    let owner = state.seats()[0];
    let primal = state.pets(Seat(0))[0];
    let vers = 1.0 + m.math().derived(state, owner).versatility_pct / 100.0;
    let as_owner = EffectCtx {
        caster: owner,
        target: None,
        spell: None,
        aura: None,
        event_amount: None,
        scale: 1.0,
        depth: 0,
    };
    let totem_want = m.math().outgoing(
        state,
        &as_owner,
        Coefficient::SpellPower(0.1),
        Outgoing::Damage(SchoolMask::FIRE),
    ) * m.math().crit_multiplier(state, &as_owner);
    assert!(
        totem_want >= 0.1 * SPELL_POWER * vers * 1.5 * 2.0,
        "the owner's own modifiers"
    );

    let (_, trace) = f.rollout(7, &[FIRE_ELEMENTAL, LIQUID_MAGMA_TOTEM]);
    // The owner's unscoped crit reaches every kind; DamageDonePct and
    // crit damage only reach totems.
    let check = |actors: Vec<ActorId>, want: f64, what: &str| {
        let hits: Vec<_> = actors.iter().flat_map(|&a| damage_by(&trace, a)).collect();
        assert!(hits.len() > 5, "{what}: {} hits", hits.len());
        for (_, _, crit, amount) in hits {
            assert!(crit, "{what}");
            assert_eq!(amount, whole_points(want), "{what}: {want}");
        }
    };
    check(
        lifetimes(&trace, FIRE_ELEMENTAL_PET).into_keys().collect(),
        0.5 * SPELL_POWER * vers * 1.1 * 2.0,
        "guardian",
    );
    check(vec![primal], SPELL_POWER * vers * 1.2 * 2.0, "pet");
    check(
        lifetimes(&trace, LIQUID_MAGMA_TOTEM_PET)
            .into_keys()
            .collect(),
        totem_want,
        "totem",
    );
}

#[test]
fn hunter_pets_autocast_and_answer_command_spells() {
    let mut f = fixture();
    with_hunter_pet(&mut f);
    f.add_spell(spell(
        KILL_COMMAND_HIT,
        CastKind::Instant,
        None,
        vec![Effect::Damage {
            amount: Coefficient::AttackPower(2.0),
            school: SchoolMask::PHYSICAL,
            target: EffectTarget::Target,
            aoe: None,
        }],
    ));
    let mut kill_command = spell(
        KILL_COMMAND,
        CastKind::Instant,
        gcd(),
        vec![Effect::CommandPet {
            pet: Some(HUNTER_PET),
            spell: KILL_COMMAND_HIT,
        }],
    );
    kill_command.cooldown = cooldown(7500);
    f.add_ability(kill_command);
    let k = f.kernel(6);
    let hunter = k.state().pets(Seat(0))[0];
    assert!(
        !k.legal(Seat(0)).abilities.contains_key(&BITE),
        "pet abilities aren't the owner's to press"
    );

    let (outcome, trace) = f.rollout(6, &[KILL_COMMAND]);
    let bites = completed(&trace, BITE);
    assert!(bites.iter().all(|&(_, actor)| actor == hunter));
    let times: Vec<SimTime> = bites.iter().map(|b| b.0).collect();
    assert!(
        gaps(&times).all(|(_, _, gap)| gap >= 3000),
        "the pet's cooldown"
    );
    let secs = f64::from((outcome.end_time - times[0]).millis()) / 1000.0;
    let affordable = (100.0 + 5.0 * secs) / 50.0;
    let n = bites.len() as f64;
    assert!(
        n <= affordable + 1.0 && n >= affordable - 3.0,
        "{n} vs {affordable}"
    );
    let commands = completed(&trace, KILL_COMMAND).len();
    let hits = damage_by(&trace, hunter)
        .iter()
        .filter(|h| h.1 == Some(KILL_COMMAND_HIT))
        .count();
    assert!(commands > 10);
    assert_eq!(hits, commands, "each Kill Command is the pet's hit");

    let mut f = fixture();
    with_hunter_pet(&mut f);
    f.template.permanent_pet = None;
    let mut call = spell(
        CALL_PET,
        CastKind::Instant,
        gcd(),
        vec![Effect::Summon {
            pet: HUNTER_PET,
            count: 1,
            duration: None,
        }],
    );
    call.targeting = Targeting::SelfOnly;
    call.cooldown = cooldown(600_000);
    f.add_ability(call);
    let (_, trace) = f.rollout_with(6, |k, seat| {
        let call = k.state().now() >= SimTime::ZERO + ms(10_000);
        priority(k, seat, if call { &[CALL_PET] } else { &[] })
    });
    let called = completed(&trace, CALL_PET)[0].0;
    let bites = completed(&trace, BITE);
    assert!(bites.len() > 5);
    assert!(bites.iter().all(|&(t, _)| t >= called));
}
