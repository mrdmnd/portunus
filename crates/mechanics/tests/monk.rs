//! Integration tests for Windwalker Monk (`SpecId(269)`) and the Conduit of
//! the Celestials (`HeroTreeId(64)`) and Shado-Pan (`HeroTreeId(65)`) Hero
//! Talent trees.

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
        self.until(
            |_| Choice::Wait(Wait::NextEvent),
            |k| !matches!(k.state().phase(SEAT), SeatPhase::Committed),
        );
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

fn hits(trace: &[TraceRecord], spell: SpellId) -> usize {
    trace
        .iter()
        .filter(
            |r| matches!(r.event, TraceEvent::Damage(d) if d.spell == Some(spell) && d.amount > 0),
        )
        .count()
}

#[test]
fn windwalker_celestial_channels_celestial_conduit_and_triggers_unity_within() {
    let f = fixture_with("monk_windwalker_celestial.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(123904)); // Invoke Xuen, the White Tiger
    d.press(SpellId(1249625)); // Zenith -> grants Zenith Stomp Ready + 2 Chi
    d.press(SpellId(443028)); // Celestial Conduit -> channels & triggers Unity Within on end
    assert!(
        buff(&d.k, AuraId(443421)) >= 1,
        "Heart of the Jade Serpent active"
    );

    d.press(SpellId(1249626)); // Zenith Stomp
    d.press(SpellId(392983)); // Strike of the Windlord
    d.press(SpellId(113656)); // Fists of Fury -> enables Whirling Dragon Punch
    d.press(SpellId(152175)); // Whirling Dragon Punch
    d.press(SpellId(100780)); // Tiger Palm
    d.press(SpellId(107428)); // Rising Sun Kick

    assert!(
        hits(&d.trace, SpellId(443028)) > 0,
        "Celestial Conduit dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(443590)) > 0,
        "Unity Within dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(123996)) > 0,
        "Xuen dealt Crackling Tiger Lightning damage"
    );
}

#[test]
fn windwalker_shado_pan_stacks_combo_strikes_and_discharges_flurry_strikes() {
    let f = fixture_with("monk_windwalker_shadopan.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(1249625)); // Zenith
    d.press(SpellId(1249626)); // Zenith Stomp
    d.press(SpellId(392983)); // Strike of the Windlord
    d.press(SpellId(100780)); // Tiger Palm
    d.press(SpellId(113656)); // Fists of Fury
    d.press(SpellId(152175)); // Whirling Dragon Punch
    d.press(SpellId(100780)); // Tiger Palm
    d.press(SpellId(107428)); // Rising Sun Kick
    d.press(SpellId(100784)); // Blackout Kick

    assert!(
        buff(&d.k, AuraId(196741)) >= 4,
        "Hit Combo stacked from alternating Combo Strikes"
    );
    assert!(
        hits(&d.trace, SpellId(451021)) > 0,
        "Flurry Strikes discharged"
    );
    assert!(
        hits(&d.trace, SpellId(451022)) > 0,
        "Wisdom of the Wall proc dealt damage"
    );
}
