//! A fixed, indexed action space for learned policies.

use std::collections::BTreeSet;

use portunus_core::{SpecId, SpellId};
use portunus_engine::{ActionMask, CastOpts, Choice, MoveGoal, TargetSel, Wait};
use portunus_gamedata::spell::CastKind;
use portunus_gamedata::talent::Grant;
use portunus_gamedata::GameData;
use serde::{Deserialize, Serialize};

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
pub trait ActionSpace: Send + Sync {
    fn size(&self) -> usize;
    /// `None` if the index points past what the mask lists.
    fn decode(&self, index: usize, mask: &ActionMask) -> Option<Choice>;
    /// `None` if the choice isn't representable in this space.
    fn encode(&self, choice: &Choice, mask: &ActionMask) -> Option<usize>;
    /// Which indices are legal right now.
    fn legal(&self, mask: &ActionMask) -> Vec<bool>;
}

/// One slot in a [`SpecActionSpace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionSlot {
    Cast {
        ability: SpellId,
        target: TargetSel,
        opts: CastOpts,
    },
    WaitNextEvent,
    WaitGcdEnd,
    WaitChannelTick,
    StopCast,
    ClearDemands,
    StopMove,
    Retarget(usize),
    CancelAura(usize),
}

/// Default number of enemy target slots in [`SpecActionSpace::for_spec`].
pub const DEFAULT_TARGET_SLOTS: usize = 8;
/// Default number of cancelable-aura slots in [`SpecActionSpace::for_spec`].
pub const DEFAULT_CANCEL_SLOTS: usize = 4;

/// A fixed [`ActionSpace`] built from every spell a spec can cast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecActionSpace {
    slots: Vec<ActionSlot>,
}

impl SpecActionSpace {
    /// Builds the canonical action space for `spec` from `data`, with
    /// [`DEFAULT_TARGET_SLOTS`] retarget slots and [`DEFAULT_CANCEL_SLOTS`]
    /// cancel-aura slots.
    pub fn for_spec(data: &GameData, spec: SpecId) -> Option<Self> {
        Self::for_spec_with(data, spec, DEFAULT_TARGET_SLOTS, DEFAULT_CANCEL_SLOTS)
    }

    /// Builds the canonical action space for `spec` with custom retarget and
    /// cancel-aura slot capacities.
    pub fn for_spec_with(
        data: &GameData,
        spec: SpecId,
        max_targets: usize,
        max_cancels: usize,
    ) -> Option<Self> {
        let spells = spec_spells(data, spec)?;
        let mut slots = Vec::new();
        for spell in spells {
            let stages = data.spells.get(&spell).and_then(|def| match &def.cast {
                CastKind::Empower { stages, .. } => u8::try_from(stages.len()).ok(),
                _ => None,
            });
            if let Some(n) = stages {
                for stage in 1..=n {
                    slots.push(ActionSlot::Cast {
                        ability: spell,
                        target: TargetSel::Primary,
                        opts: CastOpts {
                            empower: Some(stage),
                            tick_wakes: false,
                        },
                    });
                }
            } else {
                slots.push(ActionSlot::Cast {
                    ability: spell,
                    target: TargetSel::Primary,
                    opts: CastOpts::default(),
                });
            }
        }
        Self::append_control_slots(&mut slots, max_targets, max_cancels);
        Some(Self { slots })
    }

    /// Builds an action space directly from an explicit spell list (each as a
    /// plain primary-target cast) plus control slots.
    pub fn from_spells(
        spells: impl IntoIterator<Item = SpellId>,
        max_targets: usize,
        max_cancels: usize,
    ) -> Self {
        let mut unique = BTreeSet::new();
        let mut slots: Vec<ActionSlot> = spells
            .into_iter()
            .filter(|&s| unique.insert(s))
            .map(|ability| ActionSlot::Cast {
                ability,
                target: TargetSel::Primary,
                opts: CastOpts::default(),
            })
            .collect();
        Self::append_control_slots(&mut slots, max_targets, max_cancels);
        Self { slots }
    }

    fn append_control_slots(slots: &mut Vec<ActionSlot>, max_targets: usize, max_cancels: usize) {
        slots.push(ActionSlot::WaitNextEvent);
        slots.push(ActionSlot::WaitGcdEnd);
        slots.push(ActionSlot::WaitChannelTick);
        slots.push(ActionSlot::StopCast);
        slots.push(ActionSlot::ClearDemands);
        slots.push(ActionSlot::StopMove);
        for i in 0..max_targets {
            slots.push(ActionSlot::Retarget(i));
        }
        for i in 0..max_cancels {
            slots.push(ActionSlot::CancelAura(i));
        }
    }

    pub fn slots(&self) -> &[ActionSlot] {
        &self.slots
    }

    fn slot_is_legal(slot: &ActionSlot, mask: &ActionMask) -> bool {
        match slot {
            ActionSlot::Cast { ability, .. } => mask.is_ready(*ability),
            ActionSlot::WaitNextEvent => true,
            ActionSlot::WaitChannelTick => mask.can_wait_tick,
            ActionSlot::StopCast => mask.can_stop,
            ActionSlot::StopMove => mask.can_stop_move,
            ActionSlot::Retarget(i) => *i < mask.targets.len(),
            ActionSlot::CancelAura(i) => *i < mask.cancel.len(),
            ActionSlot::WaitGcdEnd | ActionSlot::ClearDemands => false,
        }
    }
}

/// Every spell `spec` can ever have in its action bar: baseline abilities,
/// spec-tree talents, hero-tree talents, and class-pull spells, in sorted
/// `SpellId` order.
pub fn spec_spells(data: &GameData, spec: SpecId) -> Option<Vec<SpellId>> {
    let spec_def = data.specs.get(&spec)?;
    let mut spells: BTreeSet<SpellId> = spec_def.baseline_spells.iter().copied().collect();
    if let Some(class) = data.classes.get(&spec_def.class) {
        for pull in &class.on_pull {
            if let Some(s) = pull.spell {
                spells.insert(s);
            }
        }
    }
    let hero_trees = data
        .hero_trees
        .values()
        .filter(|h| h.specs.contains(&spec))
        .map(|h| &h.tree);
    for node in spec_def
        .trees
        .iter()
        .chain(hero_trees)
        .flat_map(|t| &t.nodes)
    {
        for &talent_id in &node.choices {
            if let Some(talent) = data.talents.get(&talent_id) {
                for rank in &talent.ranks {
                    for eff in rank {
                        match eff {
                            Grant::Spell(s) => {
                                spells.insert(*s);
                            }
                            Grant::ReplaceSpell { to, .. } => {
                                spells.insert(*to);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    Some(spells.into_iter().collect())
}

impl ActionSpace for SpecActionSpace {
    fn size(&self) -> usize {
        self.slots.len()
    }

    fn decode(&self, index: usize, mask: &ActionMask) -> Option<Choice> {
        match self.slots.get(index)? {
            ActionSlot::Cast {
                ability,
                target,
                opts,
            } => Some(Choice::Cast {
                ability: *ability,
                target: *target,
                opts: *opts,
            }),
            ActionSlot::WaitNextEvent => Some(Choice::Wait(Wait::NextEvent)),
            ActionSlot::WaitGcdEnd => Some(Choice::Wait(Wait::GcdEnd)),
            ActionSlot::WaitChannelTick => Some(Choice::Wait(Wait::ChannelTick)),
            ActionSlot::StopCast => Some(Choice::StopCast),
            ActionSlot::ClearDemands => Some(Choice::Move(MoveGoal::ClearDemands)),
            ActionSlot::StopMove => Some(Choice::StopMove),
            ActionSlot::Retarget(i) => mask.targets.get(*i).copied().map(Choice::SetTarget),
            ActionSlot::CancelAura(i) => mask.cancel.get(*i).copied().map(Choice::CancelAura),
        }
    }

    fn encode(&self, choice: &Choice, mask: &ActionMask) -> Option<usize> {
        let wanted = match choice {
            Choice::Cast {
                ability,
                target,
                opts,
            } => {
                let exact = ActionSlot::Cast {
                    ability: *ability,
                    target: *target,
                    opts: *opts,
                };
                if let Some(idx) = self.slots.iter().position(|s| *s == exact) {
                    return Some(idx);
                }
                // Fall back to matching the ability on primary target (e.g.
                // when an empower spell was cast with `empower: None`).
                return self.slots.iter().position(|s| {
                    matches!(
                        s,
                        ActionSlot::Cast {
                            ability: a,
                            target: t,
                            ..
                        } if a == ability && t == target
                    )
                });
            }
            Choice::Wait(Wait::NextEvent) => ActionSlot::WaitNextEvent,
            Choice::Wait(Wait::GcdEnd) => ActionSlot::WaitGcdEnd,
            Choice::Wait(Wait::ChannelTick) => ActionSlot::WaitChannelTick,
            Choice::Wait(_) => return None,
            Choice::StopCast => ActionSlot::StopCast,
            Choice::Move(MoveGoal::ClearDemands) => ActionSlot::ClearDemands,
            Choice::Move(_) => return None,
            Choice::StopMove => ActionSlot::StopMove,
            Choice::SetTarget(actor) => {
                let i = mask.targets.iter().position(|a| a == actor)?;
                ActionSlot::Retarget(i)
            }
            Choice::CancelAura(aura) => {
                let i = mask.cancel.iter().position(|a| a == aura)?;
                ActionSlot::CancelAura(i)
            }
        };
        self.slots.iter().position(|s| *s == wanted)
    }

    fn legal(&self, mask: &ActionMask) -> Vec<bool> {
        self.slots
            .iter()
            .map(|slot| Self::slot_is_legal(slot, mask))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use portunus_core::{ActorId, AuraId, SimDuration};
    use portunus_engine::Readiness;

    use super::*;

    #[test]
    fn spec_action_space_round_trips_casts_waits_targets_and_cancels() {
        let space = SpecActionSpace::from_spells([SpellId(100), SpellId(200)], 2, 2);
        let mask = ActionMask {
            abilities: BTreeMap::from([
                (SpellId(100), Readiness::Now),
                (SpellId(200), Readiness::In(SimDuration(1500))),
            ]),
            can_stop: true,
            can_wait_tick: true,
            targets: vec![ActorId(10), ActorId(20)],
            cancel: vec![AuraId(500)],
            can_move: true,
            can_stop_move: false,
        };

        let legal = space.legal(&mask);
        assert!(legal[0], "Spell 100 ready");
        assert!(!legal[1], "Spell 200 on cooldown");

        for choice in [Choice::cast(SpellId(100)), Choice::Wait(Wait::ChannelTick)] {
            let idx = space.encode(&choice, &mask).expect("encodes");
            assert!(legal[idx], "{choice:?} should be legal at {idx}");
            assert_eq!(space.decode(idx, &mask), Some(choice));
        }

        // Control slots round-trip through encode/decode.
        for choice in [
            Choice::Wait(Wait::NextEvent),
            Choice::Wait(Wait::GcdEnd),
            Choice::StopCast,
            Choice::Move(MoveGoal::ClearDemands),
            Choice::SetTarget(ActorId(20)),
            Choice::CancelAura(AuraId(500)),
        ] {
            let idx = space.encode(&choice, &mask).expect("encodes");
            assert_eq!(space.decode(idx, &mask), Some(choice));
        }

        // Cancel slot 1 has no second cancelable aura in the mask.
        let cancel_1_idx = space
            .slots()
            .iter()
            .position(|s| *s == ActionSlot::CancelAura(1))
            .unwrap();
        assert!(!legal[cancel_1_idx]);
        assert_eq!(space.decode(cancel_1_idx, &mask), None);
    }
}
