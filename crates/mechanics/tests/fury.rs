//! The checked-in Fury Warrior data (SimC's 12.1 reference build, Slayer, and
//! Mountain Thane) through the real mechanics and kernel.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, MoveGoal, Outcome, Readiness, RunSetup,
    SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{Kits, PartyMechanics};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

const SEAT: Seat = Seat(0);

const BLOODTHIRST: SpellId = SpellId(23881);
const RAGING_BLOW: SpellId = SpellId(85288);
const RAGING_BLOW_MH: SpellId = SpellId(96103);
const RAGING_BLOW_OH: SpellId = SpellId(85384);
const RAMPAGE: SpellId = SpellId(184367);
const RAMPAGE_1: SpellId = SpellId(184707);
const RAMPAGE_2: SpellId = SpellId(184709);
const RAMPAGE_3: SpellId = SpellId(201364);
const RAMPAGE_4: SpellId = SpellId(201363);
const WHIRLWIND: SpellId = SpellId(190411);
const RECKLESSNESS: SpellId = SpellId(1719);
const AVATAR: SpellId = SpellId(107574);
const BLADESTORM: SpellId = SpellId(227847);
const BLADESTORM_HIT: SpellId = SpellId(50622);
const ODYNS_FURY: SpellId = SpellId(385059);
const THUNDER_CLAP: SpellId = SpellId(6343);
const THUNDER_BLAST: SpellId = SpellId(435222);
const LIGHTNING_STRIKE: SpellId = SpellId(435791);
const ELECTRICAL_CURRENT: SpellId = SpellId(1270829);
const CHARGE: SpellId = SpellId(100);
const UNHINGED_BT: SpellId = SpellId(910202);

const ENRAGE: AuraId = AuraId(184362);
const WHIRLWIND_BUFF: AuraId = AuraId(85739);
const RECKLESSNESS_BUFF: AuraId = AuraId(1719);
const AVATAR_BUFF: AuraId = AuraId(107574);
const SLAUGHTERING_STRIKES: AuraId = AuraId(388004);
const RAMPAGING_BERSERKER_BUFF: AuraId = AuraId(1269260);
const RAMPAGING_BERSERKER_STR: AuraId = AuraId(1269266);
const OPPORTUNIST_BUFF: AuraId = AuraId(456120);
const THUNDER_BLAST_BUFF: AuraId = AuraId(435615);
const ODYNS_FURY_DOT: AuraId = AuraId(385060);
const REND_DOT: AuraId = AuraId(388539);

struct Fixture {
    data: GameData,
    enemies: EnemyData,
    template: ActorTemplate,
    spec: ScenarioSpec,
}

fn fixture_with(loadout: &str) -> Fixture {
    load_fixture(loadout, true)
}

/// The Slayer default with its hero tree removed, so core-kit tests don't
/// see Slayer procs.
fn spec_tree() -> Fixture {
    load_fixture("warrior_fury_slayer.ron", false)
}

fn load_fixture(loadout: &str, keep_hero: bool) -> Fixture {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    let enemies: EnemyData = EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    let mut loadout: Loadout = read_ron(&dir.join("loadouts").join(loadout)).unwrap();
    if !keep_hero {
        strip_hero(&data, &mut loadout);
    }
    let template = Compiler.compile(&data, &loadout).unwrap();
    let mut spec: ScenarioSpec = read_ron(&dir.join("scenarios/target_dummy.ron")).unwrap();
    spec.pulls[0].health = Dist::Fixed(30.0);
    Fixture {
        data,
        enemies,
        template,
        spec,
    }
}

fn strip_hero(data: &GameData, loadout: &mut Loadout) {
    let Some(id) = loadout.hero_tree.take() else {
        return;
    };
    let hero: std::collections::BTreeSet<_> = data.hero_trees[&id]
        .tree
        .nodes
        .iter()
        .flat_map(|node| node.choices.iter())
        .collect();
    loadout.talents.0.retain(|talent, _| !hero.contains(talent));
}

impl Fixture {
    fn kernel(&self, seed: u64) -> Kernel<PartyMechanics> {
        let enemies = Arc::new(self.enemies.clone());
        let sampler = Sampler::new(self.spec.clone(), Arc::clone(&enemies)).unwrap();
        let setup = RunSetup {
            run: Arc::new(sampler.sample(Seed(seed))),
            data: Arc::new(self.data.clone()),
            enemies,
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
        };
        let mechanics = PartyMechanics::new(&setup, Arc::new(Kits::standard())).unwrap();
        Kernel::new(setup, mechanics).unwrap()
    }
}

fn me(k: &Kernel<PartyMechanics>) -> ActorId {
    k.state().seats()[0]
}

fn boss(k: &Kernel<PartyMechanics>) -> ActorId {
    k.state().enemies()[0]
}

fn aura_on(k: &Kernel<PartyMechanics>, actor: ActorId, aura: AuraId) -> u8 {
    let me = me(k);
    k.state()
        .auras(actor)
        .iter()
        .find(|i| i.aura == aura && i.source == me)
        .map_or(0, |i| i.stacks)
}

fn buff(k: &Kernel<PartyMechanics>, aura: AuraId) -> u8 {
    aura_on(k, me(k), aura)
}

fn debuff(k: &Kernel<PartyMechanics>, aura: AuraId) -> u8 {
    aura_on(k, boss(k), aura)
}

fn rage(k: &Kernel<PartyMechanics>) -> f64 {
    k.state()
        .resource(me(k), ResourceKind::Rage)
        .map_or(0.0, |r| r.value)
}

fn ready(k: &Kernel<PartyMechanics>, ability: SpellId) -> bool {
    k.legal(SEAT).abilities.get(&ability) == Some(&Readiness::Now)
}

fn cast(ability: SpellId) -> Choice {
    Choice::Cast {
        ability,
        target: TargetSel::Primary,
        opts: CastOpts::default(),
    }
}

fn idle(k: &Kernel<PartyMechanics>) -> Choice {
    let state = k.state();
    let Some(&target) = state.enemies().first() else {
        return Choice::Wait(Wait::NextEvent);
    };
    if state
        .distance(SEAT, target)
        .is_some_and(|d| d > MELEE_RANGE)
        && state.movement(SEAT).is_none()
        && k.legal(SEAT).can_move
    {
        return Choice::Move(MoveGoal::Approach {
            target,
            within: MELEE_RANGE,
        });
    }
    Choice::Wait(Wait::NextEvent)
}

struct Driver {
    k: Kernel<PartyMechanics>,
    trace: Vec<TraceRecord>,
}

impl Driver {
    fn new(f: &Fixture, seed: u64) -> Self {
        Self {
            k: f.kernel(seed),
            trace: Vec::new(),
        }
    }

    fn until(
        &mut self,
        choose: impl Fn(&Kernel<PartyMechanics>) -> Choice,
        stop: impl Fn(&Kernel<PartyMechanics>) -> bool,
    ) -> Option<Outcome> {
        for _ in 0..200_000 {
            let step = self.k.advance().unwrap();
            self.trace.extend(self.k.drain_trace());
            match step {
                Step::Done(o) => return Some(o),
                Step::Decide(req) => {
                    assert_eq!(req.seat, SEAT);
                    if stop(&self.k) {
                        return None;
                    }
                    let choice = choose(&self.k);
                    self.k.submit(SEAT, choice).unwrap();
                }
            }
        }
        panic!("the run should end");
    }

    fn press(&mut self, ability: SpellId) -> SimTime {
        if !ready(&self.k, ability) {
            self.until(idle, |k| ready(k, ability));
        }
        assert!(ready(&self.k, ability), "{ability:?}");
        let now = self.k.state().now();
        self.k.submit(SEAT, cast(ability)).unwrap();
        self.until(|_| Choice::Wait(Wait::NextEvent), |_| true);
        now
    }

    fn in_combat(&mut self) {
        let started = |k: &Kernel<PartyMechanics>| {
            k.state().enemies().first().is_some_and(|&e| {
                k.state().actor(e).is_some_and(|a| {
                    a.engaged
                        && k.state()
                            .distance(SEAT, e)
                            .is_some_and(|d| d <= MELEE_RANGE)
                })
            })
        };
        assert!(self.until(idle, started).is_none());
    }
}

fn hits(trace: &[TraceRecord], spell: SpellId) -> Vec<(SimTime, bool, u64)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.spell == Some(spell) => Some((r.time, d.crit, d.amount)),
            _ => None,
        })
        .collect()
}

#[test]
fn fury_raging_blow_and_rampage_strike_with_both_weapons_and_enrage() {
    let f = spec_tree();
    let mut d = Driver::new(&f, 1);
    d.in_combat();
    d.press(CHARGE);
    d.press(RAGING_BLOW);
    assert_eq!(hits(&d.trace, RAGING_BLOW_MH).len(), 1);
    assert_eq!(hits(&d.trace, RAGING_BLOW_OH).len(), 1);
    // Off-hand deals half the main-hand damage when neither crits (or both crit).
    let mh = hits(&d.trace, RAGING_BLOW_MH)[0];
    let oh = hits(&d.trace, RAGING_BLOW_OH)[0];
    if mh.1 == oh.1 {
        assert!(
            (oh.2 as f64 / mh.2 as f64 - 0.5).abs() < 0.02,
            "{mh:?} {oh:?}"
        );
    }
    assert_eq!(buff(&d.k, SLAUGHTERING_STRIKES), 1);

    // Recklessness doubles rage generation and triggers Berserker's Torment Avatar.
    let before = rage(&d.k);
    d.press(RECKLESSNESS);
    assert_eq!(buff(&d.k, RECKLESSNESS_BUFF), 1);
    assert_eq!(buff(&d.k, AVATAR_BUFF), 1);
    assert_eq!(buff(&d.k, RAMPAGING_BERSERKER_BUFF), 1);

    d.press(ODYNS_FURY);
    assert_eq!(buff(&d.k, ENRAGE), 1);
    assert_eq!(debuff(&d.k, ODYNS_FURY_DOT), 1);
    assert!(rage(&d.k) >= before + 30.0);

    // Build to 80 rage and cast Rampage: 4 sub-strikes, consumes Slaughtering
    // Strikes, and grants 4 stacks of Rampaging Berserker Strength (80 rage / 20).
    while rage(&d.k) < 80.0 {
        if ready(&d.k, RAGING_BLOW) {
            d.press(RAGING_BLOW);
        } else if ready(&d.k, BLOODTHIRST) {
            d.press(BLOODTHIRST);
        } else {
            d.press(WHIRLWIND);
        }
    }
    d.press(RAMPAGE);
    assert_eq!(hits(&d.trace, RAMPAGE_1).len(), 1);
    assert_eq!(hits(&d.trace, RAMPAGE_2).len(), 1);
    assert_eq!(hits(&d.trace, RAMPAGE_3).len(), 1);
    assert_eq!(hits(&d.trace, RAMPAGE_4).len(), 1);
    assert_eq!(buff(&d.k, SLAUGHTERING_STRIKES), 0);
    assert!(buff(&d.k, RAMPAGING_BERSERKER_STR) >= 4);
}

#[test]
fn fury_whirlwind_cleaves_single_target_attacks_to_four_nearby_targets() {
    let mut f = spec_tree();
    f.template.passive_auras.push(AuraId(280772));
    f.template.passive_auras.push(AuraId(280392));
    f.spec.pulls[0].waves[0].mobs[0].1 = 5;
    let mut d = Driver::new(&f, 2);
    d.in_combat();
    d.press(CHARGE);
    d.press(WHIRLWIND);
    // Meat Cleaver + Improved Whirlwind grants 4 stacks of Whirlwind.
    assert_eq!(buff(&d.k, WHIRLWIND_BUFF), 4);

    let mark = d.trace.len();
    d.press(BLOODTHIRST);
    assert_eq!(buff(&d.k, WHIRLWIND_BUFF), 3);
    assert_eq!(hits(&d.trace[mark..], BLOODTHIRST).len(), 5);
}

#[test]
fn fury_slayer_procs_opportunist_and_unhinged_bloodthirst() {
    let f = fixture_with("warrior_fury_slayer.ron");
    let mut d = Driver::new(&f, 3);
    d.in_combat();
    d.press(CHARGE);
    d.press(RAGING_BLOW);
    assert_eq!(buff(&d.k, OPPORTUNIST_BUFF), 1);

    let t = d.press(BLADESTORM);
    let done = |k: &Kernel<PartyMechanics>| k.state().now() >= t + SimDuration(7000);
    assert!(d.until(idle, done).is_none());
    assert!(hits(&d.trace, BLADESTORM_HIT).len() >= 7);
    assert_eq!(hits(&d.trace, UNHINGED_BT).len(), 3);
}

#[test]
fn fury_mountain_thane_avatar_grants_thunder_blast_and_lightning_strike() {
    let f = fixture_with("warrior_fury_mountain_thane.ron");
    let mut d = Driver::new(&f, 4);
    d.in_combat();
    d.press(CHARGE);
    d.press(AVATAR);
    // Avatar of the Storm + Flashing Skies: 2 stacks of Thunder Blast.
    assert_eq!(buff(&d.k, THUNDER_BLAST_BUFF), 2);
    assert!(ready(&d.k, THUNDER_CLAP));

    d.press(THUNDER_CLAP);
    assert_eq!(buff(&d.k, THUNDER_BLAST_BUFF), 1);
    assert_eq!(hits(&d.trace, THUNDER_BLAST).len(), 1);
    // Flashing Skies guarantees a Lightning Strike, Conductive Shielding deals
    // Electrical Current, Crashing Thunder applies Rend, and Storm Bolts grants
    // 4 stacks of Whirlwind.
    assert!(!hits(&d.trace, LIGHTNING_STRIKE).is_empty());
    assert_eq!(hits(&d.trace, ELECTRICAL_CURRENT).len(), 1);
    assert_eq!(debuff(&d.k, REND_DOT), 1);
    assert_eq!(buff(&d.k, WHIRLWIND_BUFF), 4);
}
