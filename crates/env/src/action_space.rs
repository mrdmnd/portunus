//! A fixed, indexed action space for learned policies.

use portunus_engine::{ActionMask, Choice};

/// Maps between engine choices and indices into a model's output head:
/// casts per ability, target selector, and empower stage, plus a small menu
/// of semantic waits and the free actions (stop, retarget, cancel aura).
///
/// Casts are indexed over a fixed spell list (every spell the spec can
/// have, not just this loadout's), so an index means the same spell across
/// loadouts. Spells a loadout lacks are never legal.
///
/// Retarget and cancel-aura actions index into the mask's `targets` and
/// `cancel` lists, so encoding and decoding take the mask the decision was
/// made against.
pub trait ActionSpace {
    fn size(&self) -> usize;
    /// `None` if the index points past what the mask lists.
    fn decode(&self, index: usize, mask: &ActionMask) -> Option<Choice>;
    /// `None` if the choice isn't representable in this space.
    fn encode(&self, choice: &Choice, mask: &ActionMask) -> Option<usize>;
    /// Which indices are legal right now.
    fn legal(&self, mask: &ActionMask) -> Vec<bool>;
}
