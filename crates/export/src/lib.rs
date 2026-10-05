//! Export to a rotation-helper addon.
//!
//! A network can't run in the game's Lua at useful speed, so export is two
//! steps: distill the teacher into a small student (a decision tree or tiny
//! network), then emit Lua that rebuilds the student's observation from the
//! game API. Only students of `InfoSet::Realistic` observations are
//! exportable, since the addon can't see anything more. Because observations
//! are a function of current state only, the addon needs no history: it
//! reads the game state at each decision and nothing else.

use portunus_env::Decision;
use portunus_policy::Policy;

pub trait Distiller<O> {
    type Student;
    /// Fit a student to the teacher's choices on these decisions.
    fn distill(&self, teacher: &dyn Policy<O>, decisions: &[Decision<O>]) -> Self::Student;
    /// Share of decisions where the student agrees with the teacher.
    fn agreement(
        &self,
        student: &Self::Student,
        teacher: &dyn Policy<O>,
        decisions: &[Decision<O>],
    ) -> f64;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonTarget {
    pub addon_name: String,
    /// The game's interface version, e.g. `110205`.
    pub interface: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonBundle {
    /// `(relative path, contents)`.
    pub files: Vec<(String, String)>,
}

pub trait AddonEmitter<S> {
    fn emit(&self, student: &S, target: &AddonTarget) -> AddonBundle;
}
