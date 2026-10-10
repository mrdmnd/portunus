//! Ground effects, on made-up spells grafted onto the checked-in Elemental
//! data: an area placed at the target that reaches only the enemies inside
//! it, and a caster who counts as standing in it until it walks too far.

mod synthetic;

use portunus_core::{
    ActorId, AuraId, Dist, EnemyKey, EventName, Seat, SimDuration, SimTime, SpellId, Trigger,
};
use portunus_engine::trace::TraceEvent;
use portunus_engine::{Choice, Engine, Kernel, MoveGoal, Readiness, StateView, Step, Wait};
use portunus_gamedata::aura::{GroundDef, Periodic};
use portunus_gamedata::effect::{EffectTarget, ModKind, ModScope, Modifier, Predicate};
use portunus_gamedata::enemy::{EnemyAction, EnemyKind, EnemyRule};
use portunus_gamedata::spell::Targeting;
use portunus_mechanics::PartyMechanics;
use synthetic::*;

const DECAY: AuraId = AuraId(900_830);
const DEFILE: SpellId = SpellId(900_831);
const SIGIL: AuraId = AuraId(900_832);
const SIGIL_CAST: SpellId = SpellId(900_833);

/// The boss stands 20 yards out. Beside it: an add 8.5 yards behind, one 20
/// yards behind, and one on its spot that walks off 30 yards at 2.5s.
fn pack(f: &mut Fixture) {
    let leaves = EnemyRule {
        name: EventName("walk off".into()),
        phase: None,
        when: Trigger::Elapsed(SimDuration(2500)),
        repeat: None,
        action: EnemyAction::Reposition {
            distance: Dist::Fixed(50.0),
        },
    };
    let mut rules = Vec::new();
    for (key, distance, own) in [
        ("near", 28.5, Vec::new()),
        ("far", 40.0, Vec::new()),
        ("leaver", 20.0, vec![leaves]),
    ] {
        let mut def = f.enemies.enemies[&dummy()].clone();
        def.key = EnemyKey(key.into());
        def.kind = EnemyKind::Add;
        def.rules = own;
        f.enemies.enemies.insert(def.key.clone(), def);
        rules.push(EnemyRule {
            name: EventName(format!("summon {key}")),
            phase: None,
            when: Trigger::Now,
            repeat: None,
            action: EnemyAction::SpawnAdds {
                adds: vec![(EnemyKey(key.into()), 1)],
                distance: Some(Dist::Fixed(distance)),
                despawn_with_spawner: false,
            },
        });
    }
    f.boss(rules);
}

/// A 10s area of radius 8 on the caster, ticking 100 to every enemy each
/// second, placed by `DEFILE` at its target.
fn decay(f: &mut Fixture) {
    let mut a = aura(DECAY);
    a.duration = Some(SimDuration(10_000));
    a.ground = Some(GroundDef { radius: 8.0 });
    a.periodic = Some(Periodic {
        period: SimDuration(1000),
        hasted: false,
        partial_final_tick: false,
        effects: vec![damage(100.0, EffectTarget::AllEnemies)],
    });
    f.data.auras.insert(DECAY, a);
    add_spell(f, spell(DEFILE, vec![apply(DECAY, EffectTarget::Caster)]));
}

/// Submit each choice in turn from the start of combat, casts once they
/// are ready, then wait until `until`.
fn drive(k: &mut Kernel<PartyMechanics>, script: &[Choice], until: SimTime) {
    let mut next = 0;
    loop {
        match k.advance().unwrap() {
            Step::Done(_) => return,
            Step::Decide(req) => {
                if req.now >= until {
                    return;
                }
                let ready = |c: &Choice| match c {
                    Choice::Cast { ability, .. } => {
                        k.legal(Seat(0)).abilities.get(ability) == Some(&Readiness::Now)
                    }
                    _ => true,
                };
                let choice = match script.get(next) {
                    Some(c) if req.now >= COMBAT_START && ready(c) => {
                        next += 1;
                        c.clone()
                    }
                    Some(_) => Choice::Wait(Wait::NextEvent),
                    None => Choice::Wait(Wait::Until(until)),
                };
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
}

#[test]
fn an_area_reaches_only_the_enemies_inside_it_while_they_stay() {
    let mut f = fixture(Vec::new());
    pack(&mut f);
    decay(&mut f);
    // An area that hits once, as it ends: a sigil.
    let mut sigil = aura(SIGIL);
    sigil.duration = Some(SimDuration(2000));
    sigil.ground = Some(GroundDef { radius: 8.0 });
    sigil.on_expire = vec![damage(500.0, EffectTarget::AllEnemies)];
    f.data.auras.insert(SIGIL, sigil);
    add_spell(
        &mut f,
        spell(SIGIL_CAST, vec![apply(SIGIL, EffectTarget::Caster)]),
    );
    let mut k = f.kernel();
    play(
        &mut k,
        &[(DEFILE, 0), (SIGIL_CAST, 2)],
        COMBAT_START + SimDuration(12_000),
    );
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    // Neither the ticks nor the expiry share an id with a spell, so they
    // land as no spell; their amounts tell them apart.
    let landed = |amount: u64| -> Vec<usize> {
        (0..4)
            .map(|i| {
                trace
                    .iter()
                    .filter(|r| {
                        matches!(r.event, TraceEvent::Damage(d)
                            if d.spell.is_none() && d.target == e[i] && d.amount == amount)
                    })
                    .count()
            })
            .collect()
    };
    // Ten ticks on the boss and the add 8.5 yards off
    // (inside a radius of 8 by its own reach); none 20 yards off;
    // two on the add before it walked away.
    assert_eq!(landed(100), vec![10, 10, 0, 2]);
    assert_eq!(
        landed(500),
        vec![0, 0, 1, 0],
        "only the far add, where it was cast"
    );
}

#[test]
fn the_caster_stands_in_its_area_until_it_has_walked_past_the_radius() {
    // Strikes deal double while standing in the area.
    let mut f = fixture(Vec::new());
    f.data
        .auras
        .get_mut(&KIT)
        .unwrap()
        .modifiers
        .push(Modifier {
            scope: ModScope::Spell(STRIKE),
            kind: ModKind::DamageDonePct,
            value: 100.0,
            per_stack: false,
            condition: Some(Predicate::InOwnGround(DECAY)),
        });
    decay(&mut f);
    let mut k = f.kernel();
    let script = [
        Choice::cast(STRIKE),
        Choice::cast(DEFILE),
        Choice::cast(STRIKE),
        Choice::Move(MoveGoal::Yards(5.0)),
        Choice::Wait(Wait::Until(COMBAT_START + SimDuration(4000))),
        Choice::cast(STRIKE),
        Choice::Move(MoveGoal::Yards(5.0)),
        Choice::Wait(Wait::Until(COMBAT_START + SimDuration(6000))),
        Choice::cast(STRIKE),
        // Placing it again puts the caster back inside.
        Choice::cast(DEFILE),
        Choice::cast(STRIKE),
    ];
    drive(&mut k, &script, COMBAT_START + SimDuration(9000));
    let strikes = hits(&k.drain_trace(), STRIKE, k.state().enemies()[0]);
    assert_eq!(strikes, vec![1000, 2000, 2000, 1000, 2000]);
    assert_eq!(k.state().pack_position(ActorId(0)), None);
}

const CONSECRATE: SpellId = SpellId(900_834);

#[test]
fn a_self_cast_area_goes_where_the_caster_is_fighting() {
    // As totems' areas do: no spell target to place it, so it centres on
    // the enemy its holder is targeting, here the boss.
    let mut f = fixture(Vec::new());
    pack(&mut f);
    decay(&mut f);
    let mut consecrate = spell(CONSECRATE, vec![apply(DECAY, EffectTarget::Caster)]);
    consecrate.targeting = Targeting::SelfOnly;
    consecrate.hostile = false;
    add_spell(&mut f, consecrate);
    let mut k = f.kernel();
    drive(
        &mut k,
        &[Choice::cast(CONSECRATE)],
        COMBAT_START + SimDuration(5500),
    );
    let trace = k.drain_trace();
    let e = k.state().enemies().to_vec();
    let ticks: Vec<usize> = (0..4)
        .map(|i| {
            trace
                .iter()
                .filter(|r| matches!(r.event, TraceEvent::Damage(d) if d.target == e[i]))
                .count()
        })
        .collect();
    assert_eq!(ticks, vec![5, 5, 0, 2]);
}
