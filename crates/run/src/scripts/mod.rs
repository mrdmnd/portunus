//! Hand-written policies over [`SeatObs`].
//!
//! A policy is a plain function from a decision to a choice. Run files
//! name one per seat; [`by_name`] resolves the name. Each spec's policies
//! live in its own module; to add one, write the function there and list
//! it in [`SCRIPTS`].

pub mod affliction;
pub mod arcane;
pub mod arms;
pub mod assassination;
pub mod beast_mastery;
pub mod demonology;
pub mod destruction;
pub mod elemental;
pub mod enhancement;
pub mod fire;
pub mod frost_dk;
pub mod frost_mage;
pub mod fury;
pub mod marksmanship;
pub mod outlaw;
pub mod retribution;
pub mod shadow;
pub mod subtlety;
pub mod survival;
pub mod unholy_dk;
pub mod windwalker;

use std::sync::Arc;

use portunus_core::{AuraId, SimDuration, SpellId};
use portunus_engine::{CastOpts, Choice, TargetSel, Wait};
use portunus_env::Decision;
use portunus_policy::Policy;

pub use affliction::affliction;
pub use arcane::arcane;
pub use arms::arms;
pub use assassination::assassination;
pub use beast_mastery::beast_mastery;
pub use demonology::demonology;
pub use destruction::destruction;
pub use elemental::elemental;
pub use enhancement::enhancement;
pub use fire::fire;
pub use frost_dk::frost_dk;
pub use frost_mage::frost_mage;
pub use fury::fury;
pub use marksmanship::marksmanship;
pub use outlaw::outlaw;
pub use retribution::retribution;
pub use shadow::shadow;
pub use subtlety::subtlety;
pub use survival::survival;
pub use unholy_dk::unholy_dk;
pub use windwalker::windwalker;

use crate::observe::SeatObs;

type Script = fn(&Decision<SeatObs>) -> Choice;

/// Every named policy.
pub const SCRIPTS: &[(&str, Script)] = &[
    ("elemental", elemental),
    ("enhancement", enhancement),
    ("arms", arms),
    ("fury", fury),
    ("retribution", retribution),
    ("frost_dk", frost_dk),
    ("unholy_dk", unholy_dk),
    ("beast_mastery", beast_mastery),
    ("marksmanship", marksmanship),
    ("survival", survival),
    ("assassination", assassination),
    ("outlaw", outlaw),
    ("subtlety", subtlety),
    ("arcane", arcane),
    ("fire", fire),
    ("frost_mage", frost_mage),
    ("affliction", affliction),
    ("demonology", demonology),
    ("destruction", destruction),
    ("shadow", shadow),
    ("windwalker", windwalker),
    ("idle", idle),
];

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
