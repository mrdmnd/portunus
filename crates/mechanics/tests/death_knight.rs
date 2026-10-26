//! The checked-in Frost and Unholy Death Knight data (SimC's 12.1 reference
//! builds for Deathbringer, Rider of the Apocalypse, and San'layn) through the
//! real mechanics and kernel.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, MoveGoal, Outcome, Readiness, RunSetup,
    SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::stats::ResourceKind;
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{Kits, PartyMechanics};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

const SEAT: Seat = Seat(0);

// Frost Spells
const FROST_STRIKE: SpellId = SpellId(49143);
const OBLITERATE: SpellId = SpellId(49020);
const HOWLING_BLAST: SpellId = SpellId(49184);
const GLACIAL_ADVANCE_DAMAGE: SpellId = SpellId(195975);
const PILLAR_OF_FROST: SpellId = SpellId(51271);
const EMPOWER_RUNE_WEAPON: SpellId = SpellId(47568);
const FROSTWYRMS_FURY: SpellId = SpellId(279302);
const FROSTWYRMS_FURY_RECALL: SpellId = SpellId(1265384);
const REAPERS_MARK: SpellId = SpellId(439843);
const REAPERS_MARK_EXPLOSION: SpellId = SpellId(436304);
const WAVE_OF_SOULS_HIT: SpellId = SpellId(435802);
const EXTERMINATE_FIRST: SpellId = SpellId(441424);
const EXTERMINATE_SECOND: SpellId = SpellId(441426);
const TROLLBANE_OBLITERATE: SpellId = SpellId(445507);

// Unholy Spells
const RAISE_DEAD: SpellId = SpellId(46584);
const OUTBREAK: SpellId = SpellId(77575);
const FESTERING_STRIKE: SpellId = SpellId(85948);
const FESTERING_SCYTHE: SpellId = SpellId(458128);
const SCOURGE_STRIKE: SpellId = SpellId(55090);
const VAMPIRIC_STRIKE: SpellId = SpellId(433895);
const INFLICTION_OF_SORROW_HIT: SpellId = SpellId(434144);
const DEATH_COIL: SpellId = SpellId(47541);
const NECROTIC_COIL: SpellId = SpellId(1242174);
const PUTREFY: SpellId = SpellId(1247378);
const PUTREFY_STRIKE: SpellId = SpellId(1277016);
const PUTREFY_EXPLOSION: SpellId = SpellId(390220);
const DARK_TRANSFORMATION: SpellId = SpellId(1233448);
const BLIGHTFALL: SpellId = SpellId(1271967);
const ARMY_OF_THE_DEAD: SpellId = SpellId(42650);
const CORRUPTED_BLOOD: SpellId = SpellId(434574);
const BLOOD_ERUPTION: SpellId = SpellId(434246);
const RUPTURED_VISCERA: SpellId = SpellId(1247379);

// Auras
const KILLING_MACHINE_BUFF: AuraId = AuraId(51124);
const PILLAR_OF_FROST_BUFF: AuraId = AuraId(51271);
const FROZEN_DOMINION_RW_BUFF: AuraId = AuraId(1233152);
const FROST_FEVER_DOT: AuraId = AuraId(55095);
const REAPERS_MARK_DEBUFF: AuraId = AuraId(434765);
const WAVE_OF_SOULS_DEBUFF: AuraId = AuraId(443404);
const EXTERMINATE_BUFF: AuraId = AuraId(441416);
const KILLING_STREAK_BUFF: AuraId = AuraId(1230916);
const CHOSEN_OF_FROSTBROOD_HASTE_BUFF: AuraId = AuraId(1265630);
const CHOSEN_OF_FROSTBROOD_FWF_BUFF: AuraId = AuraId(1265639);

const VIRULENT_PLAGUE_DOT: AuraId = AuraId(191587);
const DREAD_PLAGUE_DOT: AuraId = AuraId(1240996);
const LESSER_GHOUL_READY_BUFF: AuraId = AuraId(1254252);
const FESTERING_SCYTHE_BUFF: AuraId = AuraId(458123);
const FESTERING_SCYTHE_TT_BUFF: AuraId = AuraId(1241077);
const CLAWING_SHADOWS_BUFF: AuraId = AuraId(1241569);
const DARK_TRANSFORMATION_BUFF: AuraId = AuraId(1235391);
const FORBIDDEN_KNOWLEDGE_BUFF: AuraId = AuraId(1242223);
const BLIGHTFALL_BUFF: AuraId = AuraId(1271975);
const GIFT_OF_THE_SANLAYN_BUFF: AuraId = AuraId(434153);
const VAMPIRIC_STRIKE_BUFF: AuraId = AuraId(433899);
const ESSENCE_OF_THE_BLOOD_QUEEN_BUFF: AuraId = AuraId(433925);
const INCITE_TERROR_DEBUFF: AuraId = AuraId(458478);

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

fn rp(k: &Kernel<PartyMechanics>) -> f64 {
    k.state()
        .resource(me(k), ResourceKind::RunicPower)
        .map_or(0.0, |r| r.value)
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
fn frost_deathbringer_reapers_mark_exterminate_and_killing_machine_work() {
    let f = fixture_with("death_knight_frost_deathbringer.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    // Pillar of Frost activates Frozen Dominion's Remorseless Winter.
    d.press(PILLAR_OF_FROST);
    assert_eq!(buff(&d.k, PILLAR_OF_FROST_BUFF), 1);
    assert_eq!(buff(&d.k, FROZEN_DOMINION_RW_BUFF), 1);

    // Reaper's Mark applies Reaper's Mark debuff, grants Killing Machine (Grim Reaper)
    // and Exterminate (Echoing Fury), and launches Wave of Souls.
    d.press(REAPERS_MARK);
    assert!(debuff(&d.k, REAPERS_MARK_DEBUFF) >= 1);
    assert!(buff(&d.k, KILLING_MACHINE_BUFF) >= 1);
    assert_eq!(buff(&d.k, EXTERMINATE_BUFF), 1);

    // Obliterate consumes Exterminate (calling down 2 scythes) and Killing Machine
    // (granting Killing Streak and firing Arctic Assault's Glacial Advance).
    d.press(OBLITERATE);
    assert_eq!(buff(&d.k, EXTERMINATE_BUFF), 0);
    assert!(buff(&d.k, KILLING_STREAK_BUFF) >= 1);

    // Empower Rune Weapon grants 40 Runic Power and Killing Machine.
    d.press(EMPOWER_RUNE_WEAPON);
    assert!(rp(&d.k) >= 40.0);
    assert!(buff(&d.k, KILLING_MACHINE_BUFF) >= 1);

    // Frost Strike spends Runic Power; Howling Blast applies Frost Fever.
    d.press(FROST_STRIKE);
    d.press(HOWLING_BLAST);
    assert_eq!(debuff(&d.k, FROST_FEVER_DOT), 1);
    assert!(debuff(&d.k, WAVE_OF_SOULS_DEBUFF) >= 1);

    // Advance until Reaper's Mark expires and verify explosion + Exterminate proc.
    d.until(idle, |k| debuff(k, REAPERS_MARK_DEBUFF) == 0);
    for s in [
        WAVE_OF_SOULS_HIT,
        EXTERMINATE_FIRST,
        EXTERMINATE_SECOND,
        GLACIAL_ADVANCE_DAMAGE,
        REAPERS_MARK_EXPLOSION,
    ] {
        assert!(!hits(&d.trace, s).is_empty(), "expected damage from {s:?}");
    }
}

#[test]
fn frost_rider_frostwyrms_fury_chosen_of_frostbrood_and_trollbane_work() {
    let f = fixture_with("death_knight_frost_rider.ron");
    let mut d = Driver::new(&f, 2);
    d.in_combat();

    // Pillar of Frost summons Trollbane via Ride or Die!
    d.press(PILLAR_OF_FROST);

    // Obliterate commands Trollbane to cast Obliterate alongside the DK (Let Terror Reign).
    d.press(OBLITERATE);

    // Frostwyrm's Fury summons all 4 Horsemen (Apocalypse Now), grants Chosen of Frostbrood
    // Haste and the Recall override buff.
    d.press(FROSTWYRMS_FURY);
    assert_eq!(buff(&d.k, CHOSEN_OF_FROSTBROOD_HASTE_BUFF), 1);
    assert_eq!(buff(&d.k, CHOSEN_OF_FROSTBROOD_FWF_BUFF), 1);

    // Pressing Frostwyrm's Fury again fires the Chosen of Frostbrood recall breath!
    d.press(FROSTWYRMS_FURY);
    assert_eq!(buff(&d.k, CHOSEN_OF_FROSTBROOD_FWF_BUFF), 0);

    let end = d.k.state().now() + SimDuration(4_000);
    d.until(idle, |k| k.state().now() >= end);
    assert!(!hits(&d.trace, TROLLBANE_OBLITERATE).is_empty());
    assert!(!hits(&d.trace, FROSTWYRMS_FURY_RECALL).is_empty());
}

#[test]
fn unholy_rider_plagues_festering_scythe_army_and_putrefy_work() {
    let f = fixture_with("death_knight_unholy_rider.ron");
    let mut d = Driver::new(&f, 3);
    d.in_combat();

    d.press(RAISE_DEAD);

    // Outbreak applies Virulent Plague and Dread Plague.
    d.press(OUTBREAK);
    assert_eq!(debuff(&d.k, VIRULENT_PLAGUE_DOT), 1);
    assert_eq!(debuff(&d.k, DREAD_PLAGUE_DOT), 1);

    // Festering Strike grants Lesser Ghoul Ready stacks and enables Festering Scythe.
    d.press(FESTERING_STRIKE);
    assert!(buff(&d.k, LESSER_GHOUL_READY_BUFF) >= 3);
    assert_eq!(buff(&d.k, FESTERING_SCYTHE_BUFF), 1);

    // Second Festering Strike press executes Festering Scythe.
    d.press(FESTERING_STRIKE);
    assert_eq!(buff(&d.k, FESTERING_SCYTHE_BUFF), 0);
    assert_eq!(buff(&d.k, FESTERING_SCYTHE_TT_BUFF), 1);

    // Scourge Strike consumes 1 stack of Lesser Ghoul Ready, summons a Lesser Ghoul,
    // erupts plagues, and grants Clawing Shadows.
    d.press(SCOURGE_STRIKE);
    assert_eq!(buff(&d.k, CLAWING_SHADOWS_BUFF), 1);

    // Army of the Dead grants 40 Runic Power, summons Army Ghouls + Abomination + Magus + 4 Horsemen,
    // and activates Forbidden Knowledge (overriding Death Coil -> Necrotic Coil).
    d.press(ARMY_OF_THE_DEAD);
    assert_eq!(buff(&d.k, FORBIDDEN_KNOWLEDGE_BUFF), 1);

    // Pressing Death Coil during Forbidden Knowledge casts Necrotic Coil.
    d.press(DEATH_COIL);
    assert!(!hits(&d.trace, NECROTIC_COIL).is_empty());

    // Putrefy deals strike + explosion damage and triggers Blightburst.
    d.press(PUTREFY);
    assert!(!hits(&d.trace, PUTREFY_STRIKE).is_empty());
    assert!(!hits(&d.trace, PUTREFY_EXPLOSION).is_empty());

    // Advance 9s so Lesser Ghoul expires and triggers Ruptured Viscera (Necromancer's Cunning).
    let end = d.k.state().now() + SimDuration(9_000);
    d.until(idle, |k| k.state().now() >= end);
    assert!(!hits(&d.trace, FESTERING_SCYTHE).is_empty());
    assert!(!hits(&d.trace, RUPTURED_VISCERA).is_empty());
}

#[test]
fn unholy_sanlayn_vampiric_strike_blood_beast_and_blightfall_work() {
    let f = fixture_with("death_knight_unholy_sanlayn.ron");
    let mut d = Driver::new(&f, 4);
    d.in_combat();

    d.press(RAISE_DEAD);
    d.press(OUTBREAK);

    // Dark Transformation activates Gift of the San'layn + Vampiric Strike, summons Blood Beast
    // (The Blood is Life), and grants Blightfall.
    d.press(DARK_TRANSFORMATION);
    assert_eq!(buff(&d.k, DARK_TRANSFORMATION_BUFF), 1);
    assert_eq!(buff(&d.k, GIFT_OF_THE_SANLAYN_BUFF), 1);
    assert_eq!(buff(&d.k, VAMPIRIC_STRIKE_BUFF), 1);
    assert_eq!(buff(&d.k, BLIGHTFALL_BUFF), 1);

    // Pressing Scourge Strike while Vampiric Strike is up casts Vampiric Strike, granting
    // Essence of the Blood Queen, Incite Terror, and Infliction of Sorrow.
    d.press(SCOURGE_STRIKE);
    assert_eq!(buff(&d.k, ESSENCE_OF_THE_BLOOD_QUEEN_BUFF), 1);
    assert_eq!(debuff(&d.k, INCITE_TERROR_DEBUFF), 1);
    assert!(!hits(&d.trace, VAMPIRIC_STRIKE).is_empty());
    assert!(!hits(&d.trace, INFLICTION_OF_SORROW_HIT).is_empty());

    // Pressing Dark Transformation again while Blightfall is active casts Blightfall,
    // consuming plagues for burst damage and resetting Putrefy.
    d.press(DARK_TRANSFORMATION);
    assert_eq!(buff(&d.k, BLIGHTFALL_BUFF), 0);
    assert!(!hits(&d.trace, BLIGHTFALL).is_empty());

    // Advance 11s so Blood Beast cleaves with Corrupted Blood and explodes with Blood Eruption.
    let end = d.k.state().now() + SimDuration(11_000);
    d.until(idle, |k| k.state().now() >= end);
    assert!(!hits(&d.trace, CORRUPTED_BLOOD).is_empty());
    assert!(!hits(&d.trace, BLOOD_ERUPTION).is_empty());
}
