//! Hand-written policies over [`SeatObs`].
//!
//! A policy is a plain function from a decision to a choice. Run files
//! name one per seat; [`by_name`] resolves the name. Each spec's policies
//! live in its own module; to add one, write the function there and list
//! it in [`SCRIPTS`].

pub mod arms;
pub mod elemental;

use std::sync::Arc;

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{CastOpts, Choice, TargetSel, Wait};
use portunus_env::Decision;
use portunus_policy::Policy;

pub use arms::arms;
pub use elemental::elemental;

use crate::observe::SeatObs;

type Script = fn(&Decision<SeatObs>) -> Choice;

/// Every named policy.
pub const SCRIPTS: &[(&str, Script)] = &[("elemental", elemental), ("arms", arms), ("idle", idle)];

/// Candidates under tuning, looked up like [`SCRIPTS`].
const CANDIDATES: &[&[(&str, Script)]] = &[arms::CANDIDATES];

pub fn by_name(name: &str) -> Option<Arc<dyn Policy<SeatObs>>> {
    SCRIPTS
        .iter()
        .chain(CANDIDATES.iter().copied().flatten())
        .find(|(n, _)| *n == name)
        .map(|&(_, f)| Arc::new(f) as Arc<dyn Policy<SeatObs>>)
}

/// Give `spell`, which puts `buff` on an ally, to the living ally that
/// has done the most damage and lacks it: the usual Power Infusion
/// target. `None` out of combat, or if the spell isn't ready.
pub fn buff_top_ally(d: &Decision<SeatObs>, spell: SpellId, buff: AuraId) -> Option<Choice> {
    if d.obs.combat_time.is_none() || !d.legal.is_ready(spell) {
        return None;
    }
    let ally = d
        .obs
        .allies
        .iter()
        .filter(|a| a.alive && !a.buffs.contains_key(&buff))
        .max_by_key(|a| a.damage_done)?;
    Some(Choice::Cast {
        ability: spell,
        target: TargetSel::Actor(ally.actor),
        opts: CastOpts::default(),
    })
}

/// Never acts.
pub fn idle(_: &Decision<SeatObs>) -> Choice {
    Choice::Wait(Wait::NextEvent)
}

/// SimC's `gcd`, unhasted.
const GCD: SimDuration = SimDuration(1500);
