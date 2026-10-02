//! A fixed, indexed action space for learned policies.

use portunus_engine::{ActionMask, Choice};

/// Maps between engine choices and indices into a model's output head:
/// casts per slot, target selector, and empower stage, plus a small menu of
/// semantic waits and the free actions (stop, retarget, cancel aura).
pub trait ActionSpace {
    fn size(&self) -> usize;
    fn decode(&self, index: usize) -> Choice;
    /// `None` if the choice isn't representable in this space.
    fn encode(&self, choice: &Choice) -> Option<usize>;
    /// Which indices are legal right now.
    fn legal(&self, mask: &ActionMask) -> Vec<bool>;
}
