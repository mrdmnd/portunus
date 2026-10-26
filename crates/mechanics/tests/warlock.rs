//! Integration tests for Affliction (`SpecId(265)`), Demonology (`SpecId(266)`),
//! and Destruction (`SpecId(267)`) Warlock kits and the Soul Harvester
//! (`HeroTreeId(57)`), Hellcaller (`HeroTreeId(58)`), and Diabolist
//! (`HeroTreeId(59)`) Hero Talent trees.

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
fn affliction_hellcaller_stacks_wither_and_channels_malefic_grasp() {
    let f = fixture_with("warlock_affliction_hellcaller.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(48181)); // Haunt
    d.press(SpellId(980)); // Agony
    d.press(SpellId(445468)); // Wither
    d.press(SpellId(442726)); // Malevolence
    d.press(SpellId(205180)); // Summon Darkglare
    assert!(buff(&d.k, AuraId(205180)) >= 1, "Darkglare active");

    d.press(SpellId(1259790)); // Unstable Affliction
    d.press(SpellId(1259790)); // Unstable Affliction
    d.press(SpellId(1257052)); // Dark Harvest
    d.press(SpellId(198590)); // Drain Soul -> overridden to Malefic Grasp

    assert!(
        hits(&d.trace, SpellId(445473)) > 0,
        "Wither Burst dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1261149)) > 0,
        "Malefic Grasp dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1261986)) > 0,
        "Shadow of Nathreza dealt damage"
    );
}

#[test]
fn demonology_diabolist_advances_diabolic_ritual_and_fires_ruination() {
    let f = fixture_with("warlock_demonology_diabolist.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(264130)); // Power Siphon -> 2 Demonic Core
    d.press(SpellId(104316)); // Call Dreadstalkers -> advances Diabolic Ritual (Pit Lord) + Ruination Ready
    assert!(buff(&d.k, AuraId(434636)) >= 1, "Ruination Ready");

    d.press(SpellId(434635)); // Ruination -> grants Infernal Bolt Ready
    assert!(buff(&d.k, AuraId(433885)) >= 1, "Infernal Bolt Ready");

    d.press(SpellId(434506)); // Infernal Bolt -> grants 3 Soul Shards
    d.press(SpellId(1250273)); // Hand of Gul'dan -> summons Wild Imps + advances Diabolic Ritual (Overlord)
    d.press(SpellId(265187)); // Summon Demonic Tyrant
    d.press(SpellId(264178)); // Demonbolt

    assert!(
        hits(&d.trace, SpellId(434635)) > 0,
        "Ruination dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(434506)) > 0,
        "Infernal Bolt dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(104318)) > 0,
        "Wild Imps cast Fel Firebolt"
    );
}

#[test]
fn destruction_diabolist_guaranteed_chaos_bolt_crits_and_summons_infernal() {
    let f = fixture_with("warlock_destruction_diabolist.ron");
    let mut d = Driver::new(&f, 7);
    d.in_combat();

    d.press(SpellId(1122)); // Summon Infernal
    d.press(SpellId(152108)); // Cataclysm -> applies Immolate
    d.press(SpellId(6353)); // Soul Fire
    d.press(SpellId(17962)); // Conflagrate -> grants Backdraft
    d.press(SpellId(116858)); // Chaos Bolt -> triggers Embers of Nihilam + Diabolic Ritual (Pit Lord) + Ruination
    d.press(SpellId(434635)); // Ruination -> grants Infernal Bolt

    let cb_crits = d
        .trace
        .iter()
        .filter(|r| {
            matches!(
                r.event,
                TraceEvent::Damage(dmg) if dmg.spell == Some(SpellId(116858)) && dmg.crit && dmg.amount > 0
            )
        })
        .count();
    assert!(cb_crits > 0, "Chaos Bolt always crits");
    assert!(
        hits(&d.trace, SpellId(1265773)) > 0,
        "Embers of Nihilam Volley dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(20153)) > 0,
        "Infernal Immolation dealt damage"
    );
}
