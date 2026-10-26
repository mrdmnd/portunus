//! The checked-in Enhancement Shaman data (SimC's 12.1 reference builds for
//! Stormbringer and Totemic) through the real mechanics and kernel.

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

const STORMSTRIKE: SpellId = SpellId(17364);
const STORMSTRIKE_MH: SpellId = SpellId(32175);
const STORMSTRIKE_OH: SpellId = SpellId(32176);
const WINDSTRIKE_MH: SpellId = SpellId(115357);
const WINDSTRIKE_OH: SpellId = SpellId(115360);
const LAVA_LASH: SpellId = SpellId(60103);
const LAVA_LASH_CATALYST: SpellId = SpellId(910310);
const LIGHTNING_BOLT: SpellId = SpellId(188196);
const CHAIN_LIGHTNING: SpellId = SpellId(188443);
const CHAIN_LIGHTNING_RTL: SpellId = SpellId(211094);
const VOLTAIC_BLAZE: SpellId = SpellId(470057);
const CRASH_LIGHTNING: SpellId = SpellId(187874);
const CRASH_LIGHTNING_CLEAVE: SpellId = SpellId(195592);
const SUNDERING: SpellId = SpellId(197214);
const SUNDERING_SPLITSTREAM: SpellId = SpellId(467283);
const PRIMORDIAL_STORM: SpellId = SpellId(1218090);
const ASCENDANCE: SpellId = SpellId(114051);
const SURGING_TOTEM: SpellId = SpellId(444995);
const TREMOR: SpellId = SpellId(455622);
const SURGING_BOLT: SpellId = SpellId(458267);
const SEARING_VOLLEY: SpellId = SpellId(458147);
const WINDFURY_ATTACK: SpellId = SpellId(25504);
const FLAMETONGUE_ATTACK: SpellId = SpellId(10444);

const MAELSTROM_WEAPON: AuraId = AuraId(344179);
const FLAME_SHOCK_DOT: AuraId = AuraId(188389);
const LASHING_FLAMES_DEBUFF: AuraId = AuraId(334168);
const CRASH_LIGHTNING_BUFF: AuraId = AuraId(187878);
const CONVERGING_STORMS_BUFF: AuraId = AuraId(198300);
const ASCENDANCE_BUFF: AuraId = AuraId(114051);
const DOOM_WINDS_BUFF: AuraId = AuraId(466772);
const TEMPEST_BUFF: AuraId = AuraId(454015);
const ARC_DISCHARGE_BUFF: AuraId = AuraId(455097);
const STORM_SWELL_BUFF: AuraId = AuraId(455089);
const UNLIMITED_POWER_BUFF: AuraId = AuraId(454394);
const HOT_HAND_BUFF: AuraId = AuraId(215785);
const PRIMORDIAL_STORM_BUFF: AuraId = AuraId(1218125);
const SURGING_ELEMENTS_BUFF: AuraId = AuraId(382043);
const SURGING_TOTEM_BUFF: AuraId = AuraId(1221347);
const AMPLIFICATION_CORE_BUFF: AuraId = AuraId(456369);
const WHIRLING_AIR: AuraId = AuraId(453409);
const WHIRLING_FIRE: AuraId = AuraId(453405);
const WHIRLING_EARTH: AuraId = AuraId(453406);
const TOTEMIC_REBOUND_BUFF: AuraId = AuraId(458269);
const SEARING_TOTEM_PET: PetId = PetId(2523);

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
fn enhancement_stormstrike_imbues_and_maelstrom_weapon_spenders() {
    let f = fixture_with("shaman_enhancement_stormbringer.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    // Voltaic Blaze applies Flame Shock and generates 3 Maelstrom Weapon stacks
    // (1 base + 2 from Fire Nova).
    d.press(VOLTAIC_BLAZE);
    assert_eq!(debuff(&d.k, FLAME_SHOCK_DOT), 1);
    assert!(buff(&d.k, MAELSTROM_WEAPON) >= 3);

    // Crash Lightning buffs weapons, Stormwell guarantees 3 Windfury Attacks,
    // and Converging Storms grants 1 stack on single target.
    d.press(CRASH_LIGHTNING);
    assert_eq!(buff(&d.k, CRASH_LIGHTNING_BUFF), 1);
    assert_eq!(buff(&d.k, CONVERGING_STORMS_BUFF), 1);
    assert!(hits(&d.trace, WINDFURY_ATTACK).len() >= 3);

    // Stormstrike hits with both weapons, consumes Converging Storms, procs
    // Crash Lightning cleave and Ride the Lightning Chain Lightning, and off-hand
    // triggers Flametongue Attack.
    d.press(STORMSTRIKE);
    assert_eq!(hits(&d.trace, STORMSTRIKE_MH).len(), 1);
    assert_eq!(hits(&d.trace, STORMSTRIKE_OH).len(), 1);
    assert_eq!(buff(&d.k, CONVERGING_STORMS_BUFF), 0);
    assert!(!hits(&d.trace, CRASH_LIGHTNING_CLEAVE).is_empty());
    assert!(!hits(&d.trace, CHAIN_LIGHTNING_RTL).is_empty());
    assert!(!hits(&d.trace, FLAMETONGUE_ATTACK).is_empty());

    // Ascendance transforms Stormstrike into Windstrike, unleashes Doom Winds,
    // and Descending Skies grants Tempest.
    d.press(ASCENDANCE);
    assert_eq!(buff(&d.k, ASCENDANCE_BUFF), 1);
    assert_eq!(buff(&d.k, DOOM_WINDS_BUFF), 1);
    assert!(buff(&d.k, TEMPEST_BUFF) >= 1);

    d.press(STORMSTRIKE);
    assert!(!hits(&d.trace, WINDSTRIKE_MH).is_empty());
    assert!(!hits(&d.trace, WINDSTRIKE_OH).is_empty());

    // Build to 5+ Maelstrom Weapon stacks and press Lightning Bolt (overridden by Tempest):
    // Tempest consumes Maelstrom Weapon, grants Storm Swell, Arc Discharge, and Unlimited Power.
    while buff(&d.k, MAELSTROM_WEAPON) < 5 {
        d.press(STORMSTRIKE);
    }
    d.press(LIGHTNING_BOLT);
    assert_eq!(buff(&d.k, STORM_SWELL_BUFF), 1);
    assert_eq!(buff(&d.k, ARC_DISCHARGE_BUFF), 1);
    assert!(buff(&d.k, UNLIMITED_POWER_BUFF) >= 1);

    // Arc Discharge makes the next Chain Lightning instant and fire twice.
    let mark = d.trace.len();
    d.press(CHAIN_LIGHTNING);
    assert_eq!(buff(&d.k, ARC_DISCHARGE_BUFF), 0);
    assert_eq!(hits(&d.trace[mark..], CHAIN_LIGHTNING).len(), 1);
    assert!(!hits(&d.trace[mark..], CHAIN_LIGHTNING_RTL).is_empty());
}

#[test]
fn enhancement_totemic_surging_totem_whirling_elements_and_sundering() {
    let f = fixture_with("shaman_enhancement_totemic.ron");
    let mut d = Driver::new(&f, 2);
    d.in_combat();

    d.press(VOLTAIC_BLAZE);
    assert_eq!(debuff(&d.k, FLAME_SHOCK_DOT), 1);

    // Surging Totem pulses Tremor on arrival, applies Amplification Core,
    // Whirling Air/Fire/Earth motes, and Primal Catalyst (Elemental Overflow).
    d.press(SURGING_TOTEM);
    assert_eq!(buff(&d.k, SURGING_TOTEM_BUFF), 1);
    assert_eq!(buff(&d.k, AMPLIFICATION_CORE_BUFF), 1);
    assert_eq!(buff(&d.k, WHIRLING_AIR), 1);
    assert_eq!(buff(&d.k, WHIRLING_FIRE), 1);
    assert_eq!(buff(&d.k, WHIRLING_EARTH), 1);
    assert_eq!(hits(&d.trace, TREMOR).len(), 1);

    // Sundering consumes Whirling Earth to summon a Searing Totem, Earthsurge
    // triggers a second Tremor from Surging Totem, Surging Elements grants +15%
    // Haste and 5 Maelstrom Weapon stacks, Lashing Flames debuffs the target,
    // and Primordial Storm replaces Sundering.
    d.press(SUNDERING);
    assert_eq!(buff(&d.k, WHIRLING_EARTH), 0);
    assert_eq!(buff(&d.k, SURGING_ELEMENTS_BUFF), 1);
    assert_eq!(buff(&d.k, PRIMORDIAL_STORM_BUFF), 1);
    assert_eq!(debuff(&d.k, LASHING_FLAMES_DEBUFF), 1);
    assert!(buff(&d.k, MAELSTROM_WEAPON) >= 5);
    assert!(hits(&d.trace, TREMOR).len() >= 2);
    assert!(d.k.state().pets(SEAT).iter().any(|&p| matches!(
        d.k.state().actor(p).map(|a| a.kind),
        Some(portunus_engine::state::ActorKind::Pet {
            pet: SEARING_TOTEM_PET,
            ..
        })
    )));

    // Lava Lash consumes Whirling Fire to grant Hot Hand, commands Searing Totem
    // to fire Searing Volley, and Primal Catalyst echoes Lava Lash 500 ms later.
    let t = d.press(LAVA_LASH);
    assert_eq!(buff(&d.k, WHIRLING_FIRE), 0);
    assert_eq!(buff(&d.k, HOT_HAND_BUFF), 1);
    assert!(!hits(&d.trace, SEARING_VOLLEY).is_empty());

    let done = |k: &Kernel<PartyMechanics>| k.state().now() >= t + SimDuration(600);
    assert!(d.until(idle, done).is_none());
    assert_eq!(hits(&d.trace, LAVA_LASH_CATALYST).len(), 1);

    // Lightning Bolt consumes Whirling Air to fire 3 Surging Bolts and grant 3
    // stacks of Totemic Rebound.
    while buff(&d.k, MAELSTROM_WEAPON) < 5 {
        d.press(STORMSTRIKE);
    }
    d.press(LIGHTNING_BOLT);
    assert_eq!(buff(&d.k, WHIRLING_AIR), 0);
    assert!(buff(&d.k, TOTEMIC_REBOUND_BUFF) >= 3);
    assert!(hits(&d.trace, SURGING_BOLT).len() >= 3);

    // Primordial Storm (pressed via Sundering override) deals Fire, Frost, and
    // Lightning damage and consumes Maelstrom Weapon.
    while buff(&d.k, MAELSTROM_WEAPON) < 5 {
        if ready(&d.k, LAVA_LASH) {
            d.press(LAVA_LASH);
        } else {
            d.press(STORMSTRIKE);
        }
    }
    d.press(SUNDERING);
    assert_eq!(hits(&d.trace, PRIMORDIAL_STORM).len(), 3);
    assert_eq!(buff(&d.k, PRIMORDIAL_STORM_BUFF), 0);
    assert!(!hits(&d.trace, SUNDERING_SPLITSTREAM).is_empty());
}
