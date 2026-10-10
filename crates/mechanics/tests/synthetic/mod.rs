//! Made-up spells grafted onto the checked-in Elemental data: a seat that
//! never crits, with rage as a counter for what its listeners saw, and a
//! boss that can spawn adds at the pull.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use portunus_core::{
    ActorId, AuraId, Dist, EnemyKey, EventName, Seat, Seed, SimDuration, SimTime, SpellId, Trigger,
};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{
    CastOpts, Choice, Engine, Externals, Kernel, Latency, Readiness, RunSetup, SeatSetup,
    StateView, Step, TargetSel, TraceRecord, Wait,
};
use portunus_gamedata::aura::{AuraDef, RefreshRule};
use portunus_gamedata::effect::{
    Coefficient, Effect, EffectTarget, ListenFor, Listener, ModKind, ModScope, Modifier, ProcChance,
};
use portunus_gamedata::enemy::{EnemyAction, EnemyKind, EnemyRule};
use portunus_gamedata::spell::{CastKind, GcdDef, SpellDef, Targeting};
use portunus_gamedata::stats::{ResourceAmount, ResourceDef, ResourceKind, SchoolMask};
use portunus_gamedata::{EnemyData, GameData};
use portunus_ingest::{read_ron, EnemyDataSource, GameDataSource, RonFile};
use portunus_loadout::{ActorTemplate, Compiler, Loadout, LoadoutCompiler};
use portunus_mechanics::{Kits, PartyMechanics};
use portunus_scenario::{Sampler, ScenarioSampler, ScenarioSpec};

pub const KIT: AuraId = AuraId(900_801);
pub const STRIKE: SpellId = SpellId(900_802);
pub const MARK: AuraId = AuraId(900_803);
pub const MARKER: SpellId = SpellId(900_804);

pub const COMBAT_START: SimTime = SimTime(10_000);

pub struct Fixture {
    pub data: GameData,
    pub enemies: EnemyData,
    pub template: ActorTemplate,
    pub spec: ScenarioSpec,
}

pub fn dummy() -> EnemyKey {
    EnemyKey("target_dummy".into())
}

pub fn aura(id: AuraId) -> AuraDef {
    AuraDef {
        id,
        name: format!("aura {}", id.0),
        duration: None,
        max_stacks: 1,
        refresh: RefreshRule::Replace,
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
        persists_through_death: false,
        unique_per_source: false,
        prevents_death: None,
        ground: None,
    }
}

pub fn spell(id: SpellId, effects: Vec<Effect>) -> SpellDef {
    SpellDef {
        id,
        name: format!("spell {}", id.0),
        school: SchoolMask::PHYSICAL,
        cast: CastKind::Instant,
        gcd: Some(GcdDef {
            base: SimDuration(1000),
            hasted: false,
            floor: SimDuration(1000),
        }),
        cooldown: None,
        costs: Vec::new(),
        targeting: Targeting::Enemy,
        hostile: true,
        range: None,
        speed: None,
        min_travel: SimDuration::ZERO,
        rolls_on_impact: false,
        castable_while_moving: false,
        usable_while_casting: false,
        weapon: None,
        requires: Vec::new(),
        effects,
    }
}

pub fn listener(on: ListenFor, effects: Vec<Effect>) -> Listener {
    Listener {
        on,
        chance: ProcChance::Always,
        internal_cooldown: None,
        per_unit: false,
        shared_with: None,
        condition: None,
        effects,
    }
}

pub fn damage(amount: f64, target: EffectTarget) -> Effect {
    Effect::Damage {
        amount: Coefficient::Flat(amount),
        school: SchoolMask::PHYSICAL,
        target,
        aoe: None,
        ignores_armor: false,
        hand: None,
        per_count: None,
        unmodified: false,
    }
}

pub fn apply(aura: AuraId, target: EffectTarget) -> Effect {
    Effect::ApplyAura {
        aura,
        target,
        stacks: 1,
        duration: None,
        per_unit_spent: None,
    }
}

pub fn rage(amount: f64) -> Effect {
    Effect::Resource(ResourceAmount {
        kind: ResourceKind::Rage,
        amount,
    })
}

/// A seat with no weapon and no crits, holding `kit` as a passive aura,
/// with `STRIKE` (1000 physical to the target) and `MARKER` (applies
/// `MARK`). The boss can't die.
pub fn fixture(kit: Vec<Listener>) -> Fixture {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data");
    let mut data: GameData = GameDataSource::load(&RonFile::new(dir.join("game.ron"))).unwrap();
    data.classes.clear();
    let mut enemies: EnemyData =
        EnemyDataSource::load(&RonFile::new(dir.join("enemies.ron"))).unwrap();
    for e in enemies.enemies.values_mut() {
        e.health = 1_000_000_000_000;
    }
    let loadout: Loadout = read_ron(&dir.join("loadouts/elemental.ron")).unwrap();
    let mut template = Compiler.compile(&data, &loadout).unwrap();
    template.abilities.clear();
    template.melee = true;
    template.main_hand = None;
    template.off_hand = None;
    template.resources.push(ResourceDef {
        kind: ResourceKind::Rage,
        max: 100_000.0,
        initial: 0.0,
        regen_per_sec: 0.0,
        regen_hasted: false,
        recharge: None,
        out_of_combat: None,
    });
    let mut kit_aura = aura(KIT);
    kit_aura.modifiers = vec![Modifier {
        scope: ModScope::All,
        kind: ModKind::CritChanceAdd,
        value: -1000.0,
        per_stack: false,
        condition: None,
    }];
    kit_aura.listeners = kit;
    data.auras.insert(KIT, kit_aura);
    template.passive_auras.push(KIT);
    data.auras.insert(MARK, aura(MARK));
    for s in [
        spell(STRIKE, vec![damage(1000.0, EffectTarget::Target)]),
        spell(MARKER, vec![apply(MARK, EffectTarget::Target)]),
    ] {
        template.abilities.insert(s.id);
        data.spells.insert(s.id, s);
    }
    let mut spec: ScenarioSpec = read_ron(&dir.join("scenarios/target_dummy.ron")).unwrap();
    spec.pulls[0].health = Dist::Fixed(1.0);
    Fixture {
        data,
        enemies,
        template,
        spec,
    }
}

impl Fixture {
    /// The boss spawns one add per entry, in order, as it engages.
    pub fn adds(&mut self, adds: Vec<(&str, u64, Vec<EnemyRule>)>) {
        let mut keys = Vec::new();
        for (key, health, rules) in adds {
            let mut def = self.enemies.enemies[&dummy()].clone();
            def.key = EnemyKey(key.into());
            def.kind = EnemyKind::Add;
            def.health = health;
            def.rules = rules;
            keys.push((def.key.clone(), 1));
            self.enemies.enemies.insert(def.key.clone(), def);
        }
        self.enemies.enemies.get_mut(&dummy()).unwrap().rules = vec![EnemyRule {
            name: EventName("summon".into()),
            phase: None,
            when: Trigger::Now,
            repeat: None,
            action: EnemyAction::SpawnAdds {
                adds: keys,
                distance: None,
                despawn_with_spawner: false,
            },
        }];
    }

    /// The boss's rules, in place of any adds.
    pub fn boss(&mut self, rules: Vec<EnemyRule>) {
        self.enemies.enemies.get_mut(&dummy()).unwrap().rules = rules;
    }

    pub fn seat_max_health(&self) -> u64 {
        self.kernel().state().actor(ActorId(0)).unwrap().max_health
    }

    pub fn kernel(&self) -> Kernel<PartyMechanics> {
        let enemies = Arc::new(self.enemies.clone());
        let sampler = Sampler::new(self.spec.clone(), Arc::clone(&enemies)).unwrap();
        let setup = RunSetup {
            run: Arc::new(sampler.sample(Seed(1))),
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

/// Cast each `(spell, enemy index)` in turn as soon as it's ready, then
/// wait until `until`. Enemy 0 is the boss; its adds follow.
pub fn play(k: &mut Kernel<PartyMechanics>, script: &[(SpellId, usize)], until: SimTime) {
    let mut next = 0;
    loop {
        match k.advance().unwrap() {
            Step::Done(_) => return,
            Step::Decide(req) => {
                if req.now >= until {
                    return;
                }
                let state = k.state();
                let ready = |s| k.legal(Seat(0)).abilities.get(&s) == Some(&Readiness::Now);
                let choice = match script.get(next) {
                    Some(&(spell, i)) if req.now >= COMBAT_START && ready(spell) => {
                        next += 1;
                        Choice::Cast {
                            ability: spell,
                            target: TargetSel::Actor(state.enemies()[i]),
                            opts: CastOpts::default(),
                        }
                    }
                    Some(_) => Choice::Wait(Wait::NextEvent),
                    None => Choice::Wait(Wait::Until(until)),
                };
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
}

pub fn rage_of(k: &Kernel<PartyMechanics>) -> f64 {
    k.state()
        .resource(ActorId(0), ResourceKind::Rage)
        .unwrap()
        .value
}

pub fn deaths(trace: &[TraceRecord]) -> Vec<(SimTime, ActorId)> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Death { actor } => Some((r.time, actor)),
            _ => None,
        })
        .collect()
}

/// Damage of `spell` landed on `target`, in order.
pub fn hits(trace: &[TraceRecord], spell: SpellId, target: ActorId) -> Vec<u64> {
    trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::Damage(d) if d.spell == Some(spell) && d.target == target => Some(d.amount),
            _ => None,
        })
        .collect()
}

pub fn close(got: u64, want: f64) -> bool {
    (got as f64 - want).abs() <= 1.0
}

pub fn add_spell(f: &mut Fixture, s: SpellDef) {
    f.template.abilities.insert(s.id);
    f.data.spells.insert(s.id, s);
}
