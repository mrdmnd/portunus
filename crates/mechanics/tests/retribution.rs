//! The checked-in Retribution Paladin data (SimC's 12.1 reference builds for
//! Templar and Herald of the Sun) through the real mechanics and kernel.

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

const JUDGMENT: SpellId = SpellId(20271);
const HAMMER_OF_WRATH: SpellId = SpellId(24275);
const BLADE_OF_JUSTICE: SpellId = SpellId(184575);
const FINAL_VERDICT: SpellId = SpellId(383328);
const WAKE_OF_ASHES: SpellId = SpellId(255937);
const SEETHING_FLAMES_HIT: SpellId = SpellId(405345);
const EXECUTION_SENTENCE: SpellId = SpellId(343527);
const EXECUTION_SENTENCE_DETONATION: SpellId = SpellId(387113);
const DIVINE_TOLL: SpellId = SpellId(375576);
const DIVINE_HAMMER_TICK: SpellId = SpellId(198137);
const HAMMER_OF_LIGHT: SpellId = SpellId(427453);
const EMPYREAN_HAMMER: SpellId = SpellId(431398);
const CRUSADING_STRIKES: SpellId = SpellId(408385);
const SUNS_AVATAR_HIT: SpellId = SpellId(431911);

const GREATER_JUDGMENT_DEBUFF: AuraId = AuraId(197277);
const EXPURGATION_DOT: AuraId = AuraId(383346);
const TRUTHS_WAKE_DOT: AuraId = AuraId(403695);
const EXECUTION_SENTENCE_DEBUFF: AuraId = AuraId(343527);
const JUDGE_JURY_BUFF: AuraId = AuraId(1253174);
const AVENGING_WRATH_BUFF: AuraId = AuraId(31884);
const HAMMER_OF_LIGHT_READY: AuraId = AuraId(427441);
const SHAKE_THE_HEAVENS_BUFF: AuraId = AuraId(431536);
const UNDISPUTED_RULING_BUFF: AuraId = AuraId(432629);
const LIGHTS_DELIVERANCE_STACKS: AuraId = AuraId(433674);
const DAWNLIGHT_READY_BUFF: AuraId = AuraId(431522);
const DAWNLIGHT_DOT: AuraId = AuraId(431380);
const SOLAR_GRACE_BUFF: AuraId = AuraId(439841);
const BLESSING_OF_ANSHE_BUFF: AuraId = AuraId(445206);
const DIVINE_PURPOSE_BUFF: AuraId = AuraId(408458);

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

fn hp(k: &Kernel<PartyMechanics>) -> f64 {
    k.state()
        .resource(me(k), ResourceKind::HolyPower)
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
fn core_blade_of_justice_judgment_and_execution_sentence_work() {
    let f = fixture_with("retribution.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    // Blade of Justice generates 2 Holy Power (with Holy Blade) and applies Expurgation.
    d.press(BLADE_OF_JUSTICE);
    assert!(hp(&d.k) >= 2.0);
    assert_eq!(debuff(&d.k, EXPURGATION_DOT), 1);

    // Judgment applies 2 stacks of Greater Judgment (with Highlord's Wrath).
    d.press(JUDGMENT);
    d.until(idle, |k| debuff(k, GREATER_JUDGMENT_DEBUFF) > 0);
    assert_eq!(debuff(&d.k, GREATER_JUDGMENT_DEBUFF), 2);

    // Execution Sentence applies its debuff and grants Judge, Jury and Executioner.
    d.press(EXECUTION_SENTENCE);
    assert_eq!(debuff(&d.k, EXECUTION_SENTENCE_DEBUFF), 1);
    assert_eq!(buff(&d.k, JUDGE_JURY_BUFF), 1);

    // Final Verdict consumes Judge, Jury and Executioner to refund its 3 Holy Power,
    // and consumes 1 stack of Greater Judgment.
    let hp_before = hp(&d.k);
    d.press(FINAL_VERDICT);
    assert_eq!(buff(&d.k, JUDGE_JURY_BUFF), 0);
    assert!(hp(&d.k) >= hp_before);
    assert_eq!(debuff(&d.k, GREATER_JUDGMENT_DEBUFF), 1);

    // Advance past Execution Sentence's 8s duration and verify detonation.
    let end = d.k.state().now() + SimDuration(9_000);
    d.until(idle, |k| k.state().now() >= end);
    assert!(!hits(&d.trace, EXECUTION_SENTENCE_DETONATION).is_empty());
    assert!(!hits(&d.trace, CRUSADING_STRIKES).is_empty());
}

#[test]
fn templar_wake_of_ashes_hammer_of_light_and_empyrean_hammers_work() {
    let f = fixture_with("retribution_templar.ron");
    let mut d = Driver::new(&f, 2);
    d.in_combat();

    // Wake of Ashes generates 3 Holy Power, applies Truth's Wake + Seething Flames,
    // triggers Radiant Glory (Avenging Wrath), and grants Hammer of Light Ready.
    d.press(WAKE_OF_ASHES);
    assert!(hp(&d.k) >= 3.0);
    assert_eq!(debuff(&d.k, TRUTHS_WAKE_DOT), 1);
    assert_eq!(buff(&d.k, AVENGING_WRATH_BUFF), 1);
    assert_eq!(buff(&d.k, HAMMER_OF_LIGHT_READY), 1);

    // Cast Hammer of Light: consumes Hammer of Light Ready, triggers Shake the Heavens
    // and Undisputed Ruling, and calls down 5 Empyrean Hammers (3 base + 2 Zealous Vindication).
    d.press(HAMMER_OF_LIGHT);
    assert_eq!(buff(&d.k, HAMMER_OF_LIGHT_READY), 0);
    assert_eq!(buff(&d.k, SHAKE_THE_HEAVENS_BUFF), 1);
    assert_eq!(buff(&d.k, UNDISPUTED_RULING_BUFF), 1);

    // Divine Toll fires 3 Judgments/Hammers of Wrath (1 base + 2 Divine Exaction) and
    // activates Divine Hammer.
    d.press(DIVINE_TOLL);

    let end = d.k.state().now() + SimDuration(6_000);
    d.until(idle, |k| k.state().now() >= end);
    assert!(buff(&d.k, LIGHTS_DELIVERANCE_STACKS) >= 5);
    for s in [
        HAMMER_OF_LIGHT,
        EMPYREAN_HAMMER,
        SEETHING_FLAMES_HIT,
        DIVINE_HAMMER_TICK,
    ] {
        assert!(!hits(&d.trace, s).is_empty(), "expected damage from {s:?}");
    }
}

#[test]
fn herald_of_the_sun_dawnlight_suns_avatar_and_walk_into_light_work() {
    let f = fixture_with("retribution_herald_of_the_sun.ron");
    let mut d = Driver::new(&f, 3);
    d.in_combat();

    // Wake of Ashes with Herald of the Sun + Radiant Glory:
    // - Grants 2 stacks of Dawnlight (Ready)
    // - Grants Divine Purpose (Aurora)
    // - Grants Blessing of An'she + 2 Holy Power (Walk Into Light)
    // - Applies Dawnlight onto the boss (Sun's Avatar)
    d.press(WAKE_OF_ASHES);
    assert_eq!(buff(&d.k, DAWNLIGHT_READY_BUFF), 2);
    assert_eq!(buff(&d.k, DIVINE_PURPOSE_BUFF), 1);
    assert_eq!(buff(&d.k, BLESSING_OF_ANSHE_BUFF), 1);
    assert_eq!(debuff(&d.k, DAWNLIGHT_DOT), 1);

    // Hammer of Wrath during Avenging Wrath casts Blade of Justice (Walk Into Light).
    d.press(HAMMER_OF_WRATH);

    // Final Verdict consumes 1 stack of Dawnlight (Ready), extends/applies Dawnlight,
    // and grants Solar Grace.
    d.press(FINAL_VERDICT);
    assert_eq!(buff(&d.k, DAWNLIGHT_READY_BUFF), 1);
    assert_eq!(buff(&d.k, SOLAR_GRACE_BUFF), 1);

    let end = d.k.state().now() + SimDuration(6_000);
    d.until(idle, |k| k.state().now() >= end);
    // Sun's Avatar pulses on each Dawnlight tick.
    assert!(!hits(&d.trace, SUNS_AVATAR_HIT).is_empty());
    // Walk Into Light triggered Blade of Justice off Hammer of Wrath.
    assert!(!hits(&d.trace, BLADE_OF_JUSTICE).is_empty());
}
