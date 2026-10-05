//! Hand-written policies over [`SeatObs`].
//!
//! A policy is a plain function from a decision to a choice. Run files
//! name one per seat; [`by_name`] resolves the name. To add one, write the
//! function and list it in [`SCRIPTS`].

use std::sync::Arc;

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{Choice, Wait};
use portunus_env::Decision;
use portunus_policy::Policy;

use crate::observe::SeatObs;

type Script = fn(&Decision<SeatObs>) -> Choice;

/// Every named policy.
pub const SCRIPTS: &[(&str, Script)] = &[("elemental", elemental), ("idle", idle)];

pub fn by_name(name: &str) -> Option<Arc<dyn Policy<SeatObs>>> {
    SCRIPTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, f)| Arc::new(f) as Arc<dyn Policy<SeatObs>>)
}

/// Never acts.
pub fn idle(_: &Decision<SeatObs>) -> Choice {
    Choice::Wait(Wait::NextEvent)
}

const LIGHTNING_BOLT: SpellId = SpellId(188196);
const LAVA_BURST: SpellId = SpellId(51505);
const FLAME_SHOCK: SpellId = SpellId(188389);
const EARTH_SHOCK: SpellId = SpellId(8042);
const FLAME_SHOCK_DOT: AuraId = AuraId(188389);

/// Flame Shock inside 30% of its 18 s duration.
const PANDEMIC: SimDuration = SimDuration(5400);

/// Trimmed SimulationCraft Elemental single target: keep Flame Shock up,
/// then Lava Burst, Earth Shock, Lightning Bolt.
pub fn elemental(d: &Decision<SeatObs>) -> Choice {
    let (obs, legal) = (&d.obs, &d.legal);
    if legal.is_ready(FLAME_SHOCK) && obs.target_aura_remaining(FLAME_SHOCK_DOT) < PANDEMIC {
        return Choice::cast(FLAME_SHOCK);
    }
    [LAVA_BURST, EARTH_SHOCK, LIGHTNING_BOLT]
        .into_iter()
        .find(|&s| legal.is_ready(s))
        .map_or(Choice::Wait(Wait::NextEvent), Choice::cast)
}
