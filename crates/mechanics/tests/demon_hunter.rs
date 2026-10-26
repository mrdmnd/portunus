//! Integration tests for Havoc (`SpecId(577)`) and Devourer (`SpecId(1480)`)
//! Demon Hunter kits and the Fel-Scarred (`HeroTreeId(34)`), Aldrachi Reaver
//! (`HeroTreeId(35)`), Annihilator (`HeroTreeId(124)`), and Void-Scarred
//! (`HeroTreeId(126)`) Hero Talent trees.

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
    if k.legal(SEAT).abilities.contains_key(&SpellId(162794))
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
        let melee = self.k.legal(SEAT).abilities.contains_key(&SpellId(162794));
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
fn havoc_aldrachi_reaver_chains_reavers_glaive_into_thrill_of_the_fight() {
    let f = fixture_with("demon_hunter_havoc_aldrachi_reaver.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(370965)); // The Hunt -> grants Reaver's Glaive Ready + 50 Fury
    assert!(buff(&d.k, AuraId(442292)) >= 1, "Reaver's Glaive Ready");

    d.press(SpellId(442294)); // Reaver's Glaive -> grants Rending Strike + Glaive Flurry
    assert!(buff(&d.k, AuraId(442624)) >= 1, "Rending Strike active");
    assert!(buff(&d.k, AuraId(442625)) >= 1, "Glaive Flurry active");

    d.press(SpellId(198013)); // Eye Beam -> enters Demonic Metamorphosis
    assert!(buff(&d.k, AuraId(162264)) >= 1, "Metamorphosis active");

    d.press(SpellId(258860)); // Essence Break
    d.press(SpellId(188499)); // Blade Dance -> overridden to Death Sweep, consumes Glaive Flurry
    d.press(SpellId(162794)); // Chaos Strike -> overridden to Annihilation, consumes Rending Strike
    assert!(
        buff(&d.k, AuraId(442688)) >= 1,
        "Thrill of the Fight active"
    );

    assert!(
        hits(&d.trace, SpellId(442294)) > 0,
        "Reaver's Glaive dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(442719)) > 0,
        "Fury of the Aldrachi dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1270899)) > 0,
        "Eternal Hunt Strike dealt damage"
    );
}

#[test]
fn devourer_annihilator_channels_void_ray_and_detonates_collapsing_star_and_world_killer() {
    let f = fixture_with("demon_hunter_devourer_annihilator.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(191427)); // Metamorphosis -> overrides Consume -> Devour, Reap -> Cull
    assert!(buff(&d.k, AuraId(162264)) >= 1, "Void Metamorphosis active");

    d.press(SpellId(473662)); // Consume -> overridden to Devour (builds Soul Fragments + Collapsing Star stacks)
    d.press(SpellId(473662)); // Devour
    d.press(SpellId(473728)); // Void Ray -> grants Eradicate Ready + triggers Voidfall Meteor
    assert!(buff(&d.k, AuraId(1226034)) >= 1, "Eradicate Ready");

    d.press(SpellId(1226033)); // Eradicate -> consumes Soul Fragments
    d.press(SpellId(1221167)); // Collapsing Star -> triggers Midnight + Voidfall Meteor + World Killer

    assert!(
        hits(&d.trace, SpellId(1253306)) > 0,
        "Voidfall Meteor dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1256354)) > 0,
        "World Killer dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1242487)) > 0,
        "Midnight Detonation dealt damage"
    );
}
