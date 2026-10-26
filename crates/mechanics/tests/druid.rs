//! Integration tests for Balance (`SpecId(102)`) and Feral (`SpecId(103)`)
//! Druid kits and the Druid of the Claw (`HeroTreeId(21)`), Wildstalker
//! (`HeroTreeId(22)`), Keeper of the Grove (`HeroTreeId(23)`), and Elune's
//! Chosen (`HeroTreeId(24)`) Hero Talent trees.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, MoveGoal, Outcome, Readiness, RunSetup,
    SeatPhase, SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{Kits, PartyMechanics};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

const SEAT: Seat = Seat(0);

struct Fixture {
    data: GameData,
    enemies: EnemyData,
    template: ActorTemplate,
    spec: ScenarioSpec,
}

fn fixture_with(loadout: &str) -> Fixture {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    let enemies: EnemyData = EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    let loadout: Loadout = read_ron(&dir.join("loadouts").join(loadout)).unwrap();
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
    if k.legal(SEAT).abilities.contains_key(&SpellId(5221))
        && state
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
        self.until(
            |_| Choice::Wait(Wait::NextEvent),
            |k| !matches!(k.state().phase(SEAT), SeatPhase::Committed),
        );
        now
    }

    fn in_combat(&mut self) {
        let melee = self.k.legal(SEAT).abilities.contains_key(&SpellId(5221));
        let started = |k: &Kernel<PartyMechanics>| {
            k.state().enemies().first().is_some_and(|&e| {
                k.state().actor(e).is_some_and(|a| {
                    a.engaged
                        && (!melee
                            || k.state()
                                .distance(SEAT, e)
                                .is_some_and(|d| d <= MELEE_RANGE))
                })
            })
        };
        assert!(self.until(idle, started).is_none());
    }
}

fn hits(trace: &[TraceRecord], spell: SpellId) -> usize {
    trace
        .iter()
        .filter(
            |r| matches!(r.event, TraceEvent::Damage(d) if d.spell == Some(spell) && d.amount > 0),
        )
        .count()
}

#[test]
fn balance_keeper_summons_treants_and_triggers_dream_burst() {
    let f = fixture_with("druid_balance_keeper.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(8921)); // Moonfire
    d.press(SpellId(93402)); // Sunfire
    d.press(SpellId(205636)); // Force of Nature -> 3 Treants + Harmony of the Grove + 3 Dream Burst Ready
    assert!(
        buff(&d.k, AuraId(428735)) >= 1,
        "Harmony of the Grove active"
    );
    assert_eq!(buff(&d.k, AuraId(433833)), 3, "3 Dream Burst charges");

    d.press(SpellId(194223)); // Celestial Alignment
    d.press(SpellId(78674)); // Starsurge
    d.press(SpellId(190984)); // Wrath -> consumes Dream Burst
    d.press(SpellId(194153)); // Starfire -> consumes Dream Burst

    assert!(
        hits(&d.trace, SpellId(433832)) >= 2,
        "Dream Burst dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1261567)) > 0,
        "Ascendant Star dealt damage"
    );
}

#[test]
fn feral_druid_of_the_claw_snapshots_tigers_fury_and_unleashes_ravage() {
    let f = fixture_with("druid_feral_druidoftheclaw.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(5217)); // Tiger's Fury -> PersistentPct +15% + Chomp Ready
    assert!(buff(&d.k, AuraId(5217)) >= 1, "Tiger's Fury active");
    assert!(buff(&d.k, AuraId(1244259)) >= 1, "Chomp Ready");

    d.press(SpellId(1822)); // Rake -> snapshots Tiger's Fury PersistentPct
    d.press(SpellId(1244258)); // Chomp
    d.press(SpellId(106951)); // Berserk -> Claw Rampage grants Ravage Ready + 3 CP
    assert!(buff(&d.k, AuraId(441585)) >= 1, "Ravage Ready");

    d.press(SpellId(22568)); // Ferocious Bite -> overridden to Ravage
    d.press(SpellId(1243807)); // Frantic Frenzy -> 5 CP
    d.press(SpellId(1079)); // Rip -> 5 CP bleed

    assert!(hits(&d.trace, SpellId(441591)) > 0, "Ravage dealt damage");
    assert!(
        hits(&d.trace, SpellId(441810)) > 0,
        "Dreadful Wound bled target"
    );
    assert!(
        hits(&d.trace, SpellId(1263659)) > 0,
        "Unseen Strike dealt damage"
    );
}
