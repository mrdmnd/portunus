//! Integration tests for Devastation (`SpecId(1467)`) and Augmentation
//! (`SpecId(1473)`) Evoker kits and the Scalecommander (`HeroTreeId(36)`),
//! Flameshaper (`HeroTreeId(37)`), and Chronowarden (`HeroTreeId(38)`) Hero
//! Talent trees.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, Outcome, Readiness, RunSetup, SeatPhase,
    SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
};
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

fn cast(ability: SpellId, empower: Option<u8>) -> Choice {
    Choice::Cast {
        ability,
        target: TargetSel::Primary,
        opts: CastOpts {
            empower,
            tick_wakes: false,
        },
    }
}

fn idle(_k: &Kernel<PartyMechanics>) -> Choice {
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

    fn press(&mut self, ability: SpellId, empower: Option<u8>) -> SimTime {
        if !ready(&self.k, ability) {
            self.until(idle, |k| ready(k, ability));
        }
        assert!(ready(&self.k, ability), "{ability:?}");
        let now = self.k.state().now();
        self.k.submit(SEAT, cast(ability, empower)).unwrap();
        self.until(
            |_| Choice::Wait(Wait::NextEvent),
            |k| !matches!(k.state().phase(SEAT), SeatPhase::Committed),
        );
        now
    }

    fn in_combat(&mut self) {
        let started = |k: &Kernel<PartyMechanics>| {
            k.state()
                .enemies()
                .first()
                .is_some_and(|&e| k.state().actor(e).is_some_and(|a| a.engaged))
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
fn devastation_flameshaper_empowers_fire_breath_and_detonates_consume_flame() {
    let f = fixture_with("devastation_flameshaper.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(375087), None); // Dragonrage -> grants 2 Essence Burst
    assert!(buff(&d.k, AuraId(375087)) >= 1, "Dragonrage active");
    assert_eq!(buff(&d.k, AuraId(359618)), 2, "2 Essence Burst stacks");

    d.press(SpellId(357208), Some(1)); // Fire Breath (Stage 1) -> applies Fire Breath DoT + Burnout
    d.press(SpellId(359073), Some(1)); // Eternity Surge (Stage 1) -> Animosity extends Dragonrage
    d.press(SpellId(356995), None); // Disintegrate -> Consume Flame ticks Fire Breath DoT + Rising Fury

    assert!(
        hits(&d.trace, SpellId(444089)) > 0,
        "Consume Flame detonated"
    );
    assert!(
        hits(&d.trace, SpellId(1271688)) > 0,
        "Rising Fury Blast dealt damage"
    );
}

#[test]
fn augmentation_chronowarden_extends_ebon_might_and_triggers_chrono_flame_and_afterimage() {
    let f = fixture_with("augmentation_chronowarden.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(360827), None); // Blistering Scales
    d.press(SpellId(409311), None); // Prescience
    d.press(SpellId(395152), None); // Ebon Might
    assert!(buff(&d.k, AuraId(395152)) >= 1, "Ebon Might active");

    d.press(SpellId(403631), None); // Breath of Eons -> Temporal Wound + Temporal Burst + extends Ebon Might
    assert!(buff(&d.k, AuraId(431698)) >= 1, "Temporal Burst active");

    d.press(SpellId(357208), Some(1)); // Fire Breath -> extends Ebon Might
    d.press(SpellId(396286), Some(1)); // Upheaval -> extends Ebon Might + Essence Burst
    d.press(SpellId(361469), None); // Living Flame -> Chrono Flame
    d.press(SpellId(395160), None); // Eruption -> Afterimage + Duplicate (lets Living Flame projectile land)

    assert!(hits(&d.trace, SpellId(431878)) > 0, "Afterimage triggered");
    assert!(
        hits(&d.trace, SpellId(431443)) > 0,
        "Chrono Flame dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1259176)) > 0,
        "Duplicate Echo dealt damage"
    );
}
