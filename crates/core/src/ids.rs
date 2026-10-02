//! Identifiers.
//!
//! Two families: numeric ids for game data (matching the game client's ids
//! where one exists), and authored names for things people write in files.
//! Names double as stable keys for plans, traces, and random-draw domains.

use serde::{Deserialize, Serialize};

macro_rules! numeric_id {
    ($(#[$meta:meta])* $name:ident($inner:ty)) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub $inner);
    };
}

macro_rules! name_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
    };
}

/// Party size for a keystone group.
pub const PARTY_SIZE: usize = 5;

numeric_id!(
    /// Root of all randomness for one rollout.
    Seed(u64)
);
numeric_id!(
    /// A party position, `0..PARTY_SIZE`.
    Seat(u8)
);
numeric_id!(
    /// Any actor (player or enemy) inside one rollout's state.
    ActorId(u16)
);
numeric_id!(
    /// Index into an actor's ability list; the stable action head for policies.
    AbilitySlot(u8)
);
numeric_id!(
    /// A named random stream owned by one actor (crit rolls, one proc, ...).
    StreamKey(u16)
);

numeric_id!(SpellId(u32));
numeric_id!(AuraId(u32));
numeric_id!(ItemId(u32));
numeric_id!(ItemSetId(u32));
numeric_id!(TalentId(u32));
numeric_id!(SpecId(u16));
numeric_id!(
    /// A pet or guardian type in `GameData`.
    PetId(u32)
);

name_id!(
    /// An enemy type in `EnemyData`, e.g. `grunt`.
    EnemyKey
);
name_id!(
    /// A pull in a scenario, e.g. `opener`.
    PullName
);
name_id!(
    /// One enemy instance within a pull, e.g. `grunt#2`.
    SpawnLabel
);
name_id!(
    /// A named enemy rule or ability, e.g. `slam`.
    EventName
);
name_id!(
    /// A named code escape hatch referenced from game data, implemented by a
    /// spec kit.
    HookKey
);
