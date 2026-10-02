//! Shared vocabulary for every Portunus crate.
//!
//! Nothing here knows about any class, dungeon, or policy. Determinism rules
//! that every other crate inherits:
//!
//! - time is integer milliseconds ([`SimTime`]); no float time, no wall clock;
//! - every random draw is addressed by `(seed, domain, index)` ([`rng`]);
//! - iteration order is always defined (no `HashMap` on semantic paths).

pub mod dist;
pub mod ids;
pub mod rng;
pub mod time;
pub mod trigger;

pub use dist::{Dist, Sample};
pub use ids::{
    AbilitySlot, ActorId, AuraId, EnemyKey, EventName, HookKey, ItemId, ItemSetId, PetId, PullName,
    Seat, Seed, SpawnLabel, SpecId, SpellId, StreamKey, TalentId, PARTY_SIZE,
};
pub use rng::{Domain, KeyedRng, Purpose};
pub use time::{SimDuration, SimTime, MILLIS_PER_SEC};
pub use trigger::Trigger;
