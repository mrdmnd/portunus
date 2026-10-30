//! Integration tests for Assassination (`SpecId(259)`), Outlaw (`SpecId(260)`),
//! and Subtlety (`SpecId(261)`) Rogue and the Trickster (`HeroTreeId(51)`),
//! Fatebound (`HeroTreeId(52)`), and Deathstalker (`HeroTreeId(53)`) hero
//! trees.

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SimTime, SpellId};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, MoveGoal, Outcome, Readiness, RunSetup,
    SeatSetup, StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::spec::MELEE_RANGE;
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{CombatMath, Kits, PartyMechanics};
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
        let setup = self.setup(seed);
        let mechanics = PartyMechanics::new(&setup, Arc::new(Kits::standard())).unwrap();
        Kernel::new(setup, mechanics).unwrap()
    }

    fn mechanics(&self, seed: u64) -> PartyMechanics {
        PartyMechanics::new(&self.setup(seed), Arc::new(Kits::standard())).unwrap()
    }

    fn setup(&self, seed: u64) -> RunSetup {
        let enemies = Arc::new(self.enemies.clone());
        let sampler = Sampler::new(self.spec.clone(), Arc::clone(&enemies)).unwrap();
        RunSetup {
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
        }
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

fn hits(trace: &[TraceRecord], spell: SpellId) -> usize {
    trace
        .iter()
        .filter(
            |r| matches!(r.event, TraceEvent::Damage(d) if d.spell == Some(spell) && d.amount > 0),
        )
        .count()
}

#[test]
fn assassination_deathstalker_applies_marks_and_triggers_darkest_night() {
    let f = fixture_with("rogue_assassination_deathstalker.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    d.press(SpellId(703)); // Garrote -> applies 3 stacks of Deathstalker's Mark
    assert_eq!(
        debuff(&d.k, AuraId(457129)),
        3,
        "Deathstalker's Mark at 3 stacks"
    );

    d.press(SpellId(1329)); // Mutilate
    d.press(SpellId(1943)); // Rupture -> consumes 1 stack of Deathstalker's Mark
    assert_eq!(debuff(&d.k, AuraId(457129)), 2);

    d.press(SpellId(1329)); // Mutilate
    d.press(SpellId(32645)); // Envenom -> consumes 2nd stack
    assert_eq!(debuff(&d.k, AuraId(457129)), 1);

    d.press(SpellId(385627)); // Kingsbane -> triggers 5 Implacable Strikes (+5 CP)
    d.press(SpellId(32645)); // Envenom -> consumes final stack -> grants Darkest Night!
    assert_eq!(buff(&d.k, AuraId(457280)), 1, "Darkest Night active");

    d.press(SpellId(1329)); // Mutilate
    d.press(SpellId(32645)); // Envenom with Darkest Night -> reapplies 3 stacks of Deathstalker's Mark!
    assert_eq!(
        debuff(&d.k, AuraId(457129)),
        3,
        "Darkest Night reapplied 3 stacks"
    );
    assert!(
        hits(&d.trace, SpellId(457157)) >= 3,
        "Deathstalker's Mark dealt damage"
    );
    assert_eq!(
        hits(&d.trace, SpellId(1265794)),
        10,
        "5 Implacable Strikes (Physical + Nature)"
    );
}

#[test]
fn swift_slasher_adds_haste_to_slice_and_dice_attack_speed() {
    let f = fixture_with("rogue_assassination_deathstalker.ron");
    let m = f.mechanics(1);
    let mut d = Driver::new(&f, 1);
    d.in_combat();
    d.press(SpellId(1329));
    d.press(SpellId(315496));
    assert_eq!(buff(&d.k, AuraId(315496)), 1, "Slice and Dice is up");

    let state = d.k.state();
    let haste = m.math().haste_mult(state, me(&d.k));
    let speed = m.math().attack_speed_mult(state, me(&d.k));
    assert!(
        (speed - (1.5 + (haste - 1.0))).abs() < 1e-9,
        "SimC's 1 + 0.5 + (haste - 1): haste {haste}, speed {speed}"
    );

    let from = d.k.state().now();
    let until = from + SimDuration::from_millis(10_000);
    d.until(idle, |k| k.state().now() >= until);
    let hits: Vec<f64> = d
        .trace
        .iter()
        .filter(|r| r.time >= from && r.time < until)
        .filter(|r| matches!(r.event, TraceEvent::Damage(h) if h.spell.is_none() && h.source == me(&d.k)))
        .map(|r| r.time.millis() as f64)
        .collect();
    // Misses leave no trace, so each hand shows up as hits a whole number
    // of swing periods apart.
    let period = 1800.0 / (haste * speed);
    let on_beat = hits
        .iter()
        .filter(|&&t| {
            hits.iter().any(|&u| {
                let k = ((u - t) / period).round();
                k != 0.0 && k.abs() <= 3.0 && (u - t - k * period).abs() <= 2.0 * k.abs()
            })
        })
        .count();
    assert!(
        hits.len() >= 10 && on_beat * 5 >= hits.len() * 4,
        "{on_beat} of {} white hits sit on a {period:.0} ms beat",
        hits.len()
    );
}

#[test]
fn outlaw_trickster_builds_escalating_blade_into_coup_de_grace() {
    let f = fixture_with("rogue_outlaw_trickster.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    d.press(SpellId(13750)); // Adrenaline Rush -> grants 7 CP & Slice and Dice
    d.press(SpellId(315341)); // Between the Eyes -> spends 7 CP, Restless Blades CDR
    for _ in 0..4 {
        d.press(SpellId(193315)); // Sinister Strike -> triggers Unseen Blade -> stacks Escalating Blade
    }
    assert_eq!(
        buff(&d.k, AuraId(441424)),
        1,
        "Coup de Grace Ready after 4 Unseen Blades"
    );

    d.press(SpellId(441776)); // Coup de Grace!
    assert_eq!(buff(&d.k, AuraId(441424)), 0, "Coup de Grace consumed");
    assert!(
        hits(&d.trace, SpellId(441144)) >= 4,
        "Unseen Blade hit 4 times"
    );
    assert!(
        hits(&d.trace, SpellId(441776)) >= 1,
        "Coup de Grace dealt damage"
    );
}

#[test]
fn outlaw_fatebound_flips_coins_and_lucky_coin_triggers_both() {
    let f = fixture_with("rogue_outlaw_fatebound.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    d.press(SpellId(13750)); // Adrenaline Rush -> activates Lucky Coin (1248971) & 7 CP
    d.press(SpellId(2098)); // Dispatch -> triggers both Heads (452923) and Tails (452538) + Scoundrel Strike!

    assert!(
        buff(&d.k, AuraId(452923)) >= 1,
        "Fatebound Coin (Heads) active"
    );
    assert!(
        hits(&d.trace, SpellId(452538)) >= 1,
        "Fatebound Coin (Tails) dealt damage"
    );
    assert!(
        hits(&d.trace, SpellId(1265918)) >= 1,
        "Gravedigger Scoundrel Strike dealt damage"
    );
}

#[test]
fn subtlety_deathstalker_shadow_dance_shadowstrike_and_secret_technique() {
    let f = fixture_with("rogue_subtlety_deathstalker.ron");
    let mut d = Driver::new(&f, 1);
    d.in_combat();

    d.press(SpellId(121471)); // Shadow Blades
    d.press(SpellId(185313)); // Shadow Dance -> grants Premeditation & Lingering Darkness
    d.press(SpellId(185438)); // Shadowstrike -> applies Find Weakness & 3 stacks of Deathstalker's Mark
    assert_eq!(debuff(&d.k, AuraId(316220)), 1, "Find Weakness applied");
    assert_eq!(
        debuff(&d.k, AuraId(457129)),
        3,
        "Deathstalker's Mark applied"
    );

    // Secret Technique: the rogue's strike, then two clone strikes 1 s and
    // 1.3 s later; consumes 1 Deathstalker's Mark.
    let cast_at = d.press(SpellId(280719));
    assert_eq!(debuff(&d.k, AuraId(457129)), 2);
    let after_clones = cast_at + SimDuration(1500);
    d.until(idle, |k| k.state().now() >= after_clones);
    assert_eq!(hits(&d.trace, SpellId(280720)), 1, "the rogue's strike");
    assert_eq!(hits(&d.trace, SpellId(282449)), 2, "both clone strikes");
}
