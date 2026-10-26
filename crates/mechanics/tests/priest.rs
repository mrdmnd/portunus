//! Integration tests for Shadow Priest (`SpecId(258)`) and the Voidweaver
//! (`HeroTreeId(18)`) and Archon (`HeroTreeId(19)`) Hero Talent trees.

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
fn shadow_voidweaver_opens_entropic_rift_and_detonates_collapsing_void() {
    let f = fixture_with("priest_shadow_voidweaver.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(1227280)); // Tentacle Slam -> applies VT + SW:P
    d.press(SpellId(451234)); // Voidwraith
    d.press(SpellId(228260)); // Voidform -> grants Void Volley Ready
    d.press(SpellId(263165)); // Void Torrent -> opens Entropic Rift (8s)
    assert!(buff(&d.k, AuraId(447444)) >= 1, "Entropic Rift active");

    d.press(SpellId(8092)); // Mind Blast -> overridden to Void Blast
    d.press(SpellId(335467)); // Shadow Word: Madness
    d.press(SpellId(1242173)); // Void Volley

    for _ in 0..8 {
        if ready(&d.k, SpellId(8092)) {
            d.press(SpellId(8092));
        } else {
            d.press(SpellId(15407));
        }
    }

    assert!(
        hits(&d.trace, SpellId(450405)) > 0,
        "Void Blast dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(447448)) > 0,
        "Entropic Rift pulsed damage"
    );
    assert!(
        hits(&d.trace, SpellId(448404)) > 0,
        "Collapsing Void detonated"
    );
    assert!(
        hits(&d.trace, SpellId(199486)) > 0,
        "Psychic Link cleaved damage"
    );
}

#[test]
fn shadow_archon_pulses_halo_waves_and_channels_mind_flay_insanity() {
    let f = fixture_with("priest_shadow_archon.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(1227280)); // Tentacle Slam
    d.press(SpellId(1230339)); // Mindbender
    d.press(SpellId(228260)); // Voidform
    d.press(SpellId(120644)); // Halo -> applies Power Surge + Surge of Insanity
    assert!(buff(&d.k, AuraId(453110)) >= 1, "Power Surge active");
    assert!(buff(&d.k, AuraId(391403)) >= 1, "Surge of Insanity active");

    d.press(SpellId(15407)); // Mind Flay -> overridden to Mind Flay: Insanity
    d.press(SpellId(8092)); // Mind Blast
    d.press(SpellId(335467)); // Shadow Word: Madness
    d.press(SpellId(1242173)); // Void Volley

    for _ in 0..6 {
        if ready(&d.k, SpellId(8092)) {
            d.press(SpellId(8092));
        } else {
            d.press(SpellId(15407));
        }
    }

    assert!(
        hits(&d.trace, SpellId(120696)) >= 3,
        "Halo + Power Surge + Divine Halo waves hit"
    );
    assert!(
        hits(&d.trace, SpellId(391403)) > 0,
        "Mind Flay: Insanity dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1264098)) > 0,
        "Void Apparition dealt damage"
    );
}
