//! Shared cooldown categories: one timer and one pool of charges for every
//! spell in the category.

mod common;

use std::sync::Arc;

use portunus_core::{SimDuration, SimTime, SpellId};
use portunus_engine::state::SegmentView;
use portunus_engine::trace::TraceEvent;
use portunus_engine::{Choice, Engine, Readiness, StateView, Step, TargetSel, Wait};
use portunus_gamedata::spell::CooldownDef;

use common::*;

/// Flame Shock and Earth Shock as the seat's only abilities, both free and
/// on a 6 s cooldown with two charges, in one category if `shared`.
fn shocks(shared: bool) -> Fixture {
    let mut f = fixture();
    let data = Arc::make_mut(&mut f.data);
    for (spell, category) in [(FLAME_SHOCK, 7), (EARTH_SHOCK, if shared { 7 } else { 8 })] {
        let def = data.spells.get_mut(&spell).expect("the shock exists");
        def.costs.clear();
        def.cooldown = Some(CooldownDef {
            duration: SimDuration(6000),
            charges: 2,
            hasted: false,
            category: Some(category),
        });
    }
    f.template.abilities = [FLAME_SHOCK, EARTH_SHOCK].into();
    f
}

/// Casts started in the first 20 s of combat, as `(ms since the first,
/// spell)`. The seat casts whichever shock is ready, preferring the one it
/// didn't cast last.
fn casts(f: &Fixture) -> Vec<(u32, SpellId)> {
    let mut k = kernel(f, 1, 1, instant());
    let mut last = EARTH_SHOCK;
    let mut start: Option<SimTime> = None;
    for _ in 0..100_000 {
        match k.advance().unwrap() {
            Step::Done(_) => break,
            Step::Decide(req) => {
                if matches!(k.state().segment(), SegmentView::Combat(_)) {
                    let t0 = *start.get_or_insert(req.now);
                    if req.now > t0 + SimDuration(20_000) {
                        break;
                    }
                }
                let mask = k.legal(req.seat);
                let ready = |s: SpellId| mask.abilities[&s] == Readiness::Now;
                let other = if last == FLAME_SHOCK {
                    EARTH_SHOCK
                } else {
                    FLAME_SHOCK
                };
                let pick = [other, last].into_iter().find(|&s| ready(s));
                let choice = match pick {
                    Some(ability) => {
                        last = ability;
                        Choice::Cast {
                            ability,
                            target: TargetSel::Primary,
                            opts: Default::default(),
                        }
                    }
                    None => Choice::Wait(Wait::NextEvent),
                };
                k.submit(req.seat, choice).unwrap();
            }
        }
    }
    let trace = k.drain_trace();
    let starts: Vec<(SimTime, SpellId)> = trace
        .iter()
        .filter_map(|r| match r.event {
            TraceEvent::CastStart { spell, .. } => Some((r.time, spell)),
            _ => None,
        })
        .collect();
    let t0 = starts[0].0;
    starts
        .into_iter()
        .filter(|&(t, _)| t <= t0 + SimDuration(20_000))
        .map(|(t, s)| ((t - t0).0, s))
        .collect()
}

#[test]
fn spells_in_a_category_share_one_cooldown_and_its_charges() {
    let (fs, es) = (FLAME_SHOCK, EARTH_SHOCK);
    // Two charges between them, then one every 6 s.
    assert_eq!(
        casts(&shocks(true)),
        vec![(0, fs), (1500, es), (6000, fs), (12_000, es), (18_000, fs)]
    );
    // Apart, each has its own two charges.
    let apart = casts(&shocks(false));
    assert_eq!(apart[..4], [(0, fs), (1500, es), (3000, fs), (4500, es)]);
}
