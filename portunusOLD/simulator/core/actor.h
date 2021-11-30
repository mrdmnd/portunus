#pragma once

#include <vector>

#include "simulator/core/aura.h"

// Base class for both player characters, pets, and enemy targets.
// This class holds current state. Variables here should generally not be const.
namespace simulator {
namespace core {
class Actor {
 public:
  inline std::vector<Aura> Buffs() const { return buffs_; }
  inline std::vector<Aura> Debuffs() const { return debuffs_; }
  inline void AddBuff(const Aura& aura) { buffs_.push_back(aura); }
  inline void AddDebuff(const Aura& aura) { debuffs_.push_back(aura); }

  inline const Actor* GetTarget() const { return current_target_; }
  inline void SetTarget(const Actor* actor) { current_target_ = actor; }

 private:
  // Actors have max health, and current health.
  // We don't actually estimate enemy health directly; rather, we use the health
  // estimator passed in from the simulation encounter config.
  // I think the only relevant use of this is to determine if life_tap is ready
  // for warlocks?
  // int maximum_health_;
  // int current_health_;

  // Actors have buffs and debuffs (lists of auras).
  // Auras have a duration that is either finite or infinite.
  // Auras have stacks.
  std::vector<Aura> buffs_;
  std::vector<Aura> debuffs_;

  // Actors have a `target` for all actions, but they don't own it, and
  // shouldn't be able to mutate it. Players can "de-target" in-game, this maps
  // to setting current_target to NULLPTR.
  //
  // The lifetime of the target must exceed or equal that of the actor.
  // Players cannot modify their target directly, but this pointer is useful
  // because it allows us to get information about our current target.
  const Actor* current_target_;
};
}  // namespace core
}  // namespace simulator
