//! Integration tests for Beast Mastery (`SpecId(253)`), Marksmanship
//! (`SpecId(254)`), and Survival (`SpecId(255)`) Hunter and the Sentinel
//! (`HeroTreeId(42)`), Pack Leader (`HeroTreeId(43)`), and Dark Ranger
//! (`HeroTreeId(44)`) hero trees.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, PetId, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, MoveGoal, Outcome, Readiness, RunSetup,
    SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
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
    if k.legal(SEAT).abilities.contains_key(&SpellId(186270))
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
        self.until(|_| Choice::Wait(Wait::NextEvent), |_| true);
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
fn beast_mastery_pack_leader_spawns_pets_and_fires_stampede() {
    let f = fixture_with("beast_mastery_pack_leader.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    d.press(SpellId(19574)); // Bestial Wrath -> grants Howl + Stampede! Ready + Bloodshed
    assert_eq!(
        buff(&d.k, AuraId(471878)),
        1,
        "Howl of the Pack Leader Ready"
    );
    assert_eq!(buff(&d.k, AuraId(472742)), 1, "Stampede! Ready");

    d.press(SpellId(34026)); // Kill Command -> commands pets + triggers Stampede + Howl Beast
    d.press(SpellId(217200)); // Barbed Shot -> applies Barbed Shot DoT, Frenzy on pets, Stomp

    let summoned: Vec<PetId> = d
        .trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::PetSummoned { pet, .. } => Some(pet),
            _ => None,
        })
        .collect();
    assert!(
        summoned.contains(&PetId(165189)),
        "Primary Hunter Pet spawned"
    );
    assert!(
        summoned.contains(&PetId(165190)),
        "Animal Companion spawned"
    );
    assert!(
        hits(&d.trace, SpellId(83381)) >= 2,
        "Both pets executed Kill Command"
    );
    assert!(
        hits(&d.trace, SpellId(201594)) >= 1,
        "Stampede dealt damage"
    );
    assert!(hits(&d.trace, SpellId(201754)) >= 1, "Stomp dealt damage");
}

#[test]
fn marksmanship_sentinel_fires_aimed_shot_moonlight_chakram_and_lunar_storm() {
    let f = fixture_with("marksmanship_sentinel.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    d.press(SpellId(288613)); // Trueshot -> applies Sentinel's Mark & grants Moonlight Chakram Ready
    d.press(SpellId(19434)); // Aimed Shot -> consumes Sentinel's Mark -> triggers Lunar Storm x4 & grants Precise Shots
    d.press(SpellId(1264949)); // Moonlight Chakram -> 6 bounces + Lock and Load
    d.press(SpellId(257044)); // Rapid Fire channel
    let end = d.k.state().now() + SimDuration(3_000);
    d.until(
        |_| Choice::Wait(Wait::NextEvent),
        |k| k.state().now() >= end,
    );

    assert!(
        hits(&d.trace, SpellId(19434)) >= 1,
        "Aimed Shot dealt damage"
    );
    assert_eq!(
        hits(&d.trace, SpellId(1253733)),
        4,
        "Lunar Storm fired 4 missiles"
    );
    assert_eq!(
        hits(&d.trace, SpellId(1266081)),
        6,
        "Moonlight Chakram hit 6 times"
    );
    assert!(
        hits(&d.trace, SpellId(257045)) >= 5,
        "Rapid Fire shots dealt damage"
    );
}

#[test]
fn marksmanship_dark_ranger_fires_black_arrow_withering_fire_and_wailing_arrow() {
    let f = fixture_with("marksmanship_dark_ranger.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    d.press(SpellId(288613)); // Trueshot -> grants Withering Fire, Wailing Arrow Ready, Dark Minion
    d.press(SpellId(466930)); // Black Arrow -> triggers Bleak Powder & 4 Withering Fire Black Arrows
    d.press(SpellId(392060)); // Wailing Arrow -> deals primary + AoE Shadow damage & grants Deathblow
    let end = d.k.state().now() + SimDuration(2_000);
    d.until(
        |_| Choice::Wait(Wait::NextEvent),
        |k| k.state().now() >= end,
    );

    assert!(
        hits(&d.trace, SpellId(466930)) >= 1,
        "Black Arrow dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(467914)) >= 1,
        "Bleak Powder exploded"
    );
    assert_eq!(
        hits(&d.trace, SpellId(468037)),
        4,
        "Withering Fire + Blighted Quiver fired 4 bonus Black Arrows"
    );
    assert!(
        hits(&d.trace, SpellId(392058)) >= 1,
        "Wailing Arrow dealt damage"
    );
}

#[test]
fn survival_sentinel_consumes_tip_of_the_spear_and_upgrades_raptor_swipe() {
    let f = fixture_with("survival_sentinel.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    d.press(SpellId(259489)); // Kill Command -> grants Tip of the Spear
    assert!(buff(&d.k, AuraId(260286)) >= 1, "Tip of the Spear active");

    d.press(SpellId(186270)); // Raptor Strike -> consumes Tip of the Spear -> triggers Strike as One & grants Raptor Swipe Ready
    assert_eq!(buff(&d.k, AuraId(1273155)), 1, "Raptor Swipe Ready");

    d.press(SpellId(1250646)); // Takedown -> grants 3 stacks Tip of the Spear, Sentinel's Mark, Moonlight Chakram Ready
    d.press(SpellId(1262293)); // Raptor Swipe -> consumes Tip of the Spear -> triggers 4x Strike as One

    assert!(
        hits(&d.trace, SpellId(1262293)) >= 1,
        "Raptor Swipe dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1251779)) >= 5,
        "Strike as One triggered on Raptor Strike + 4x on Raptor Swipe"
    );
}
