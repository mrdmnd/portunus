//! Damage and cost modifiers that item effects and tier sets use, on
//! made-up auras grafted onto the checked-in Elemental data.

mod synthetic;

use portunus_core::{AuraId, SimDuration, SimTime, SpellId};
use portunus_engine::{Engine, StateView};
use portunus_gamedata::aura::Periodic;
use portunus_gamedata::effect::{EffectTarget, ModKind, ModScope, Modifier};
use portunus_gamedata::stats::{Cost, ResourceKind, SpendScaling};
use synthetic::*;

const TRINKET: SpellId = SpellId(900_971);
const SPEND: SpellId = SpellId(900_972);
const DOT: SpellId = SpellId(900_973);

fn t(ms: u32) -> SimTime {
    COMBAT_START + SimDuration(ms)
}

fn passive(f: &mut Fixture, scope: ModScope, kind: ModKind, value: f64) {
    let kit = f.data.auras.get_mut(&KIT).unwrap();
    kit.modifiers.push(Modifier {
        scope,
        kind,
        value,
        per_stack: false,
        condition: None,
    });
}

/// The first hit of `spell` on the boss after casting it once.
fn first_hit(f: &Fixture, spell: SpellId) -> u64 {
    let mut k = f.kernel();
    play(&mut k, &[(spell, 0)], t(1500));
    let boss = k.state().enemies()[0];
    hits(&k.drain_trace(), spell, boss)[0]
}

#[test]
fn ability_modifiers_pass_item_effects_by() {
    let mut f = fixture(Vec::new());
    let mut trinket = spell(TRINKET, vec![damage(1000.0, EffectTarget::Target)]);
    trinket.item = true;
    add_spell(&mut f, trinket);
    passive(&mut f, ModScope::Abilities, ModKind::DamageDonePct, 100.0);
    passive(&mut f, ModScope::All, ModKind::DamageDonePct, 50.0);
    let strike = first_hit(&f, STRIKE) as f64;
    let item = first_hit(&f, TRINKET) as f64;
    assert!((strike / item - 2.0).abs() < 1e-3, "{strike} vs {item}");
}

/// The direct hit and first tick of a spell that hits and leaves a DoT of
/// the same id, under `mods` on that id.
fn hit_and_tick(mods: &[(ModKind, f64)]) -> (f64, f64) {
    let mut f = fixture(Vec::new());
    let mut dot = aura(AuraId(DOT.0));
    dot.duration = Some(SimDuration(3000));
    dot.periodic = Some(Periodic {
        period: SimDuration(1000),
        hasted: false,
        partial_final_tick: false,
        tick_zero: false,
        effects: vec![damage(1000.0, EffectTarget::Target)],
    });
    f.data.auras.insert(dot.id, dot);
    add_spell(
        &mut f,
        spell(
            DOT,
            vec![
                damage(1000.0, EffectTarget::Target),
                apply(AuraId(DOT.0), EffectTarget::Target),
            ],
        ),
    );
    for &(kind, value) in mods {
        passive(&mut f, ModScope::Spell(DOT), kind, value);
    }
    let mut k = f.kernel();
    play(&mut k, &[(DOT, 0)], t(2500));
    let boss = k.state().enemies()[0];
    let got = hits(&k.drain_trace(), DOT, boss);
    (got[0] as f64, got[1] as f64)
}

#[test]
fn direct_and_periodic_modifiers_reach_their_own_hits() {
    let (hit, tick) = hit_and_tick(&[]);
    let (hit2, tick2) = hit_and_tick(&[
        (ModKind::DirectDamagePct, 100.0),
        (ModKind::PeriodicDamagePct, 50.0),
    ]);
    assert!((hit2 / hit - 2.0).abs() < 1e-3, "{hit2} vs {hit}");
    assert!((tick2 / tick - 1.5).abs() < 1e-3, "{tick2} vs {tick}");
    let (hit3, tick3) = hit_and_tick(&[(ModKind::DamageDonePct, 100.0)]);
    assert!((hit3 / hit - 2.0).abs() < 1e-3 && (tick3 / tick - 2.0).abs() < 1e-3);
}

#[test]
fn damage_grows_with_the_targets_missing_health() {
    const HEALTH: u64 = 4000;
    let mut f = fixture(Vec::new());
    f.enemies.enemies.get_mut(&dummy()).unwrap().health = HEALTH;
    passive(
        &mut f,
        ModScope::Spell(STRIKE),
        ModKind::DamagePerTargetHealthMissingPct,
        2.0,
    );
    let mut k = f.kernel();
    play(&mut k, &[(STRIKE, 0), (STRIKE, 0)], t(3000));
    let boss = k.state().enemies()[0];
    let got = hits(&k.drain_trace(), STRIKE, boss);
    let first = got[0] as f64;
    let missing_pct = 100.0 * first / HEALTH as f64;
    let want = first * (1.0 + 0.02 * missing_pct);
    assert!((got[1] as f64 - want).abs() <= 1.0, "{got:?} vs {want}");
}

#[test]
fn flat_cost_modifiers_apply_before_percent_ones() {
    let mut f = fixture(Vec::new());
    for r in &mut f.template.resources {
        if r.kind == ResourceKind::Rage {
            r.initial = 100.0;
        }
    }
    let mut spend = spell(SPEND, Vec::new());
    spend.costs = vec![Cost {
        kind: ResourceKind::Rage,
        amount: 30.0,
        extra: 0.0,
        scaling: SpendScaling::None,
    }];
    add_spell(&mut f, spend);
    passive(
        &mut f,
        ModScope::Spell(SPEND),
        ModKind::ResourceCostFlat(ResourceKind::Rage),
        -10.0,
    );
    passive(&mut f, ModScope::Spell(SPEND), ModKind::CostPct, -50.0);
    let mut k = f.kernel();
    play(&mut k, &[(SPEND, 0)], t(500));
    assert!((rage_of(&k) - 90.0).abs() < 1e-9, "{}", rage_of(&k));
}
