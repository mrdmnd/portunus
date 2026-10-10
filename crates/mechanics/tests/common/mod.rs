//! Running the checked-in Elemental data through the real mechanics and
//! kernel, and reading the traces back.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::{CastEndReason, TraceEvent};
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, Outcome, Readiness, RunSetup, SeatSetup,
    StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::aura::AuraDef;
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{Kits, PartyMechanics};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

pub const LIGHTNING_BOLT: SpellId = SpellId(188196);
pub const LAVA_BURST: SpellId = SpellId(51505);
pub const FLAME_SHOCK: SpellId = SpellId(188389);
pub const EARTH_SHOCK: SpellId = SpellId(8042);
pub const STORMKEEPER: SpellId = SpellId(191634);
pub const ANCESTRAL_SWIFTNESS: SpellId = SpellId(443454);
pub const FLAME_SHOCK_DOT: AuraId = AuraId(188389);

pub struct Fixture {
    pub data: GameData,
    pub enemies: Arc<EnemyData>,
    pub template: ActorTemplate,
    pub scenario: ScenarioSpec,
}

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

/// The game data, minus classes' group buffs, with a loadout from
/// `data/loadouts`, edited by `edit`.
pub fn fixture(loadout: &str, edit: impl FnOnce(&mut Loadout)) -> Fixture {
    let mut data: GameData = GameDataSource::load(&RonFile::new(dir().join("game.ron"))).unwrap();
    // The kits are tested on their own, without the class's group buffs.
    data.classes.clear();
    let enemies: EnemyData =
        EnemyDataSource::load(&RonFile::new(dir().join("enemies.ron"))).unwrap();
    let mut loadout: Loadout = read_ron(&dir().join("loadouts").join(loadout)).unwrap();
    edit(&mut loadout);
    let template = Compiler.compile(&data, &loadout).unwrap();
    let scenario: ScenarioSpec = read_ron(&dir().join("scenarios/target_dummy.ron")).unwrap();
    Fixture {
        data,
        enemies: Arc::new(enemies),
        template,
        scenario,
    }
}

impl Fixture {
    pub fn setup(&self, seed: u64) -> RunSetup {
        let sampler = Sampler::new(self.scenario.clone(), Arc::clone(&self.enemies)).unwrap();
        RunSetup {
            run: Arc::new(sampler.sample(Seed(seed))),
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

    pub fn kernel(&self, seed: u64) -> Kernel<PartyMechanics> {
        let setup = self.setup(seed);
        let mechanics = PartyMechanics::new(&setup, Arc::new(Kits::standard())).unwrap();
        Kernel::new(setup, mechanics).unwrap()
    }

    pub fn mechanics(&self, seed: u64) -> PartyMechanics {
        PartyMechanics::new(&self.setup(seed), Arc::new(Kits::standard())).unwrap()
    }

    pub fn rollout(&self, seed: u64) -> Vec<TraceRecord> {
        let mut k = self.kernel(seed);
        play_until(&mut k, |_| false).expect("the run ends");
        k.drain_trace()
    }
}

/// Stormkeeper, Ancestral Swiftness while Lava Burst is ready, Flame Shock
/// if missing, Lava Burst, Earth Shock, Lightning Bolt.
pub fn priority(k: &Kernel<PartyMechanics>, seat: Seat) -> Choice {
    let mask = k.legal(seat);
    let ready = |s: SpellId| mask.abilities.get(&s) == Some(&Readiness::Now);
    let state = k.state();
    let me = state.seats()[usize::from(seat.0)];
    let dot_up = state
        .target(me)
        .is_some_and(|t| state.auras(t).iter().any(|a| a.aura == FLAME_SHOCK_DOT));
    let pick = [STORMKEEPER]
        .into_iter()
        .chain(ready(LAVA_BURST).then_some(ANCESTRAL_SWIFTNESS))
        .chain((!dot_up).then_some(FLAME_SHOCK))
        .chain([LAVA_BURST, EARTH_SHOCK, LIGHTNING_BOLT])
        .find(|&s| ready(s));
    match pick {
        Some(ability) => Choice::Cast {
            ability,
            target: TargetSel::Primary,
            opts: CastOpts::default(),
        },
        None => Choice::Wait(Wait::NextEvent),
    }
}

/// Play with `priority` until the run ends or `stop` holds before a
/// decision; `Some` if the run ended.
pub fn play_until(
    k: &mut Kernel<PartyMechanics>,
    stop: impl Fn(&Kernel<PartyMechanics>) -> bool,
) -> Option<Outcome> {
    for _ in 0..200_000 {
        match k.advance().unwrap() {
            Step::Done(o) => return Some(o),
            Step::Decide(req) => {
                if stop(k) {
                    return None;
                }
                let choice = priority(k, req.seat);
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    panic!("the run should end");
}

/// Stacks of `aura` on seat 0.
pub fn holds(k: &Kernel<PartyMechanics>, aura: AuraId) -> u8 {
    let state = k.state();
    state
        .auras(state.seats()[0])
        .iter()
        .find(|a| a.aura == aura)
        .map_or(0, |a| a.stacks)
}

/// Replays a trace's aura changes, so each record can be read against the
/// stacks held just before it.
#[derive(Default)]
pub struct Stacks(BTreeMap<(ActorId, AuraId), u8>);

impl Stacks {
    pub fn get(&self, holder: ActorId, aura: AuraId) -> u8 {
        self.0.get(&(holder, aura)).copied().unwrap_or(0)
    }

    pub fn apply(&mut self, event: &TraceEvent) {
        match *event {
            TraceEvent::AuraApplied {
                holder,
                aura,
                stacks,
            } => {
                self.0.insert((holder, aura), stacks);
            }
            TraceEvent::AuraRemoved { holder, aura } => {
                self.0.remove(&(holder, aura));
            }
            _ => {}
        }
    }
}

/// When casts of `want` completed, by anyone.
pub fn completed(trace: &[TraceRecord], want: SpellId) -> Vec<SimTime> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::CastEnd {
                spell,
                reason: CastEndReason::Completed,
                ..
            } if spell == want => Some(r.time),
            _ => None,
        })
        .collect()
}

pub fn aura(id: AuraId, duration: Option<SimDuration>) -> AuraDef {
    AuraDef {
        id,
        name: format!("test aura {}", id.0),
        duration,
        max_stacks: 1,
        refresh: Default::default(),
        periodic: None,
        value: None,
        modifiers: Vec::new(),
        listeners: Vec::new(),
        overrides: Vec::new(),
        on_expire: Vec::new(),
        cancelable: false,
        blocked_by: None,
        form: None,
        stealth: None,
        ends_with: None,
    }
}
