//! Integration tests for Arcane (`SpecId(62)`), Fire (`SpecId(63)`), and
//! Frost (`SpecId(64)`) Mage kits and the Sunfury (`HeroTreeId(39)`),
//! Spellslinger (`HeroTreeId(40)`), and Frostfire (`HeroTreeId(41)`) Hero
//! Talent trees.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, Outcome, Readiness, RunSetup, SeatPhase,
    SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::stats::ResourceKind;
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
fn arcane_spellslinger_builds_salvo_splinters_and_detonates_touch_of_the_magi() {
    let f = fixture_with("mage_arcane_spellslinger.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(365350)); // Arcane Surge
    assert!(buff(&d.k, AuraId(365362)) >= 1, "Arcane Surge buff up");

    d.press(SpellId(321507)); // Touch of the Magi
    assert!(
        (d.k.state()
            .resource(me(&d.k), ResourceKind::ArcaneCharges)
            .unwrap()
            .value
            - 4.0)
            .abs()
            < 1e-6,
        "Touch of the Magi grants 4 Arcane Charges"
    );

    d.press(SpellId(5143)); // Arcane Missiles -> builds Arcane Salvo
    d.press(SpellId(153626)); // Arcane Orb -> builds Arcane Salvo
    d.press(SpellId(44425)); // Arcane Barrage -> triggers Prismatic Bolt Ready
    d.press(SpellId(1295924)); // Prismatic Bolt

    for _ in 0..8 {
        if ready(&d.k, SpellId(5143)) {
            d.press(SpellId(5143));
        } else {
            d.press(SpellId(30451));
        }
    }

    assert!(
        hits(&d.trace, SpellId(443763)) > 0,
        "Arcane Splinters dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(210833)) > 0,
        "Touch of the Magi detonated"
    );
}

#[test]
fn fire_sunfury_chains_combustion_hot_streaks_and_summons_arcane_phoenix() {
    let f = fixture_with("mage_fire_sunfury.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(190319)); // Combustion
    assert!(buff(&d.k, AuraId(190319)) >= 1, "Combustion active");
    d.press(SpellId(153561)); // Meteor

    for _ in 0..10 {
        if buff(&d.k, AuraId(48108)) >= 1 {
            d.press(SpellId(11366));
        } else if ready(&d.k, SpellId(108853)) {
            d.press(SpellId(108853));
        } else {
            d.press(SpellId(2948));
        }
    }

    assert!(hits(&d.trace, SpellId(12654)) > 0, "Ignite ticked");
    assert!(
        hits(&d.trace, SpellId(449559)) + hits(&d.trace, SpellId(448662)) > 0,
        "Arcane Phoenix dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(449560)) > 0,
        "Sunfury Meteorite dealt damage"
    );
}

#[test]
fn frost_frostfire_stacks_freezing_and_fires_glacial_spike_and_frostfire_burst() {
    let f = fixture_with("mage_frost_frostfire.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(84714)); // Frozen Orb
    d.press(SpellId(1247777)); // Comet Storm
    d.press(SpellId(205021)); // Ray of Frost

    for _ in 0..10 {
        if ready(&d.k, SpellId(199786)) {
            d.press(SpellId(199786));
        } else if buff(&d.k, AuraId(190446)) >= 1 && ready(&d.k, SpellId(44614)) {
            d.press(SpellId(44614));
        } else if buff(&d.k, AuraId(44544)) >= 1 {
            d.press(SpellId(30455));
        } else {
            d.press(SpellId(431044));
        }
    }

    assert!(
        hits(&d.trace, SpellId(199786)) > 0,
        "Glacial Spike dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1246949)) > 0,
        "Freezing Shatter dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(431171)) > 0,
        "Frostfire Infusion dealt damage"
    );
}
