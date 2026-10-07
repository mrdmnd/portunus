//! Shared fixture: the checked-in Elemental data driven by a stub
//! `Mechanics` that only knows spell-power damage, auras, and resources.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{ActorId, AuraId, Dist, Seat, Seed, SimDuration, SpellId};
use portunus_engine::mechanics::{
    whole_points, AuraApplication, AuraChange, AuraEvent, CastEvent, DamageEvent, DeathEvent,
    EnemyHit, PetEvent, RolledHit, SwingEvent, TickEvent, TimerEvent,
};
use portunus_engine::state::Projectile;
use portunus_engine::{
    AuraRef, CastOpts, Choice, Engine, EngineIo, Externals, Kernel, Latency, Mechanics, Outcome,
    Readiness, RunSetup, SeatSetup, StateView, Step, TargetSel, Wait,
};
use portunus_gamedata::effect::{Coefficient, Effect, EffectTarget};
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

pub const SPELL_POWER: f64 = 3000.0;
pub const LIGHTNING_BOLT: SpellId = SpellId(188196);
pub const LAVA_BURST: SpellId = SpellId(51505);
pub const FLAME_SHOCK: SpellId = SpellId(188389);
pub const EARTH_SHOCK: SpellId = SpellId(8042);
pub const FLAME_SHOCK_DOT: AuraId = AuraId(188389);

pub struct Fixture {
    pub data: Arc<GameData>,
    pub enemies: Arc<EnemyData>,
    pub template: ActorTemplate,
    pub sampler: Sampler,
}

fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

pub fn fixture() -> Fixture {
    let dir = data_dir();
    let data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    let enemies: EnemyData = EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    let loadout: Loadout = read_ron(&dir.join("loadouts/elemental.ron")).unwrap();
    let mut template = Compiler.compile(&data, &loadout).unwrap();
    // The tests start at the pull: nothing may be castable before it.
    template.abilities.retain(|s| data.spells[s].hostile);
    let spec: ScenarioSpec = read_ron(&dir.join("scenarios/target_dummy.ron")).unwrap();
    let enemies = Arc::new(enemies);
    let sampler = Sampler::new(spec, Arc::clone(&enemies)).unwrap();
    Fixture {
        data: Arc::new(data),
        enemies,
        template,
        sampler,
    }
}

pub fn latency(reaction: Dist<SimDuration>) -> Latency {
    Latency {
        anticipated: Dist::Fixed(SimDuration::ZERO),
        reaction,
        cast_lag: Dist::Fixed(SimDuration::ZERO),
    }
}

pub fn human() -> Latency {
    latency(Dist::Jitter {
        base: SimDuration(250),
        jitter: SimDuration(50),
    })
}

pub fn instant() -> Latency {
    latency(Dist::Fixed(SimDuration::ZERO))
}

pub fn setup(f: &Fixture, seed: u64, seats: usize, latency: Latency) -> RunSetup {
    RunSetup {
        run: Arc::new(f.sampler.sample(Seed(seed))),
        data: Arc::clone(&f.data),
        enemies: Arc::clone(&f.enemies),
        seats: (0..seats)
            .map(|_| SeatSetup {
                template: f.template.clone(),
                latency: latency.clone(),
            })
            .collect(),
        externals: Externals::default(),
        record_trace: true,
    }
}

pub fn kernel(f: &Fixture, seed: u64, seats: usize, latency: Latency) -> Kernel<Stub> {
    Kernel::new(setup(f, seed, seats, latency), Stub).unwrap()
}

#[derive(Debug, Clone)]
pub struct Stub;

fn pick(t: EffectTarget, caster: ActorId, target: Option<ActorId>) -> Option<ActorId> {
    match t {
        EffectTarget::Caster => Some(caster),
        EffectTarget::Target => target,
        _ => None,
    }
}

fn run(
    io: &mut dyn EngineIo,
    caster: ActorId,
    target: Option<ActorId>,
    spell: Option<SpellId>,
    effects: &[Effect],
    scale: f64,
) {
    for e in effects {
        match e {
            Effect::Damage {
                amount: Coefficient::SpellPower(c),
                school,
                target: t,
                ..
            } => {
                if let Some(target) = pick(*t, caster, target) {
                    io.apply_damage(DamageEvent {
                        source: caster,
                        target,
                        amount: whole_points(c * SPELL_POWER * scale),
                        school: *school,
                        spell,
                        crit: false,
                    });
                }
            }
            Effect::ApplyAura {
                aura,
                target: t,
                stacks,
                duration,
            } => {
                if let Some(holder) = pick(*t, caster, target) {
                    io.apply_aura(AuraApplication {
                        aura: AuraRef {
                            holder,
                            aura: *aura,
                            source: caster,
                        },
                        stacks: *stacks,
                        duration: *duration,
                    });
                }
            }
            Effect::Resource(r) => io.add_resource(caster, r.kind, r.amount),
            _ => {}
        }
    }
}

fn land(io: &mut dyn EngineIo, cast: &CastEvent) {
    let effects = io.data().spells[&cast.spell].effects.clone();
    run(io, cast.actor, cast.target, Some(cast.spell), &effects, 1.0);
}

impl Mechanics for Stub {
    fn gate(&self, _view: &dyn StateView, _seat: Seat, _ability: SpellId) -> Readiness {
        Readiness::Now
    }
    fn combat_started(&self, _io: &mut dyn EngineIo, _combat: u16) {}
    fn combat_ended(&self, _io: &mut dyn EngineIo, _combat: u16, _cleared: bool) {}
    fn cast_started(&self, _io: &mut dyn EngineIo, _cast: &CastEvent) {}
    fn cast_completed(&self, io: &mut dyn EngineIo, cast: &CastEvent) {
        if io.data().spells[&cast.spell].travel.is_none() {
            land(io, cast);
        }
    }
    fn channel_tick(&self, _io: &mut dyn EngineIo, _cast: &CastEvent, _tick: u8) {}
    fn projectile_landed(
        &self,
        io: &mut dyn EngineIo,
        cast: &CastEvent,
        _flight: &Projectile,
        _hits: &[RolledHit],
    ) {
        land(io, cast);
    }
    fn swing(&self, _io: &mut dyn EngineIo, _swing: &SwingEvent) {}
    fn periodic_tick(&self, io: &mut dyn EngineIo, tick: &TickEvent) {
        let Some(p) = io.data().auras[&tick.aura.aura].periodic.clone() else {
            return;
        };
        let r = tick.aura;
        run(
            io,
            r.source,
            Some(r.holder),
            None,
            &p.effects,
            tick.fraction,
        );
    }
    fn aura_changed(&self, _io: &mut dyn EngineIo, _ev: &AuraChange) {}
    fn aura_removed(&self, _io: &mut dyn EngineIo, _ev: &AuraEvent) {}
    fn actor_died(&self, _io: &mut dyn EngineIo, _ev: &DeathEvent) {}
    fn pet_expired(&self, _io: &mut dyn EngineIo, _ev: &PetEvent) {}
    fn enemy_hit(&self, _io: &mut dyn EngineIo, _hit: &EnemyHit) {}
    fn timer(&self, _io: &mut dyn EngineIo, _timer: &TimerEvent) {}
}

pub fn cast(ability: SpellId) -> Choice {
    Choice::Cast {
        ability,
        target: TargetSel::Primary,
        opts: CastOpts::default(),
    }
}

/// Flame Shock if missing, then Lava Burst, Earth Shock, Lightning Bolt.
pub fn priority(k: &Kernel<Stub>, seat: Seat) -> Choice {
    let mask = k.legal(seat);
    let ready = |s: SpellId| mask.abilities.get(&s) == Some(&Readiness::Now);
    let state = k.state();
    let me = state.seats()[usize::from(seat.0)];
    let dot_up = state
        .target(me)
        .is_some_and(|t| state.auras(t).iter().any(|a| a.aura == FLAME_SHOCK_DOT));
    if ready(FLAME_SHOCK) && !dot_up {
        cast(FLAME_SHOCK)
    } else if ready(LAVA_BURST) {
        cast(LAVA_BURST)
    } else if ready(EARTH_SHOCK) {
        cast(EARTH_SHOCK)
    } else if ready(LIGHTNING_BOLT) {
        cast(LIGHTNING_BOLT)
    } else {
        Choice::Wait(Wait::NextEvent)
    }
}

/// Answer with `priority` for up to `decisions` decisions; `Some` if the run
/// ended first.
pub fn play_for(k: &mut Kernel<Stub>, decisions: usize) -> Option<Outcome> {
    for _ in 0..decisions {
        match k.advance().unwrap() {
            Step::Done(o) => return Some(o),
            Step::Decide(req) => {
                let choice = priority(k, req.seat);
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    None
}

pub fn play(k: &mut Kernel<Stub>) -> Outcome {
    play_for(k, 100_000).expect("the run should end")
}
