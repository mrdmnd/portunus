#include <vector>

#include "simulator/core/aura.h"

// Base class for both player characters, pets, and enemy targets.
// This class holds current state. Variables here should generally not be const.
namespace simulator {
namespace core {
class Actor {
 public:
  // Should not be able to actually create an actor.
  // When a derived class is constructed, the base class's constructor is called
  // as well. This allows for initialization of the base class's member
  // variables.
  Actor() = delete;
  ~Actor() = delete;

  inline std::vector<Aura> GetAuras() const { return active_auras_; }
  inline void AddAura(const Aura& aura) { active_auras_.push_back(aura); }
  inline bool HasAura(const SpellId) {
    // Do a search on the Auras vector.
    return false;
  }

  inline const Actor* GetTarget() const { return current_target_; }
  inline void SetTarget(const Actor* actor) { current_target_ = actor; }

 private:
  // Actors have max health, and current health.
  // We don't actually estimate enemy health directly; rather, we use the health
  // estimator passed in from the simulation encounter config.
  int maximum_health_;
  int current_health_;

  // Actors have auras. Auras are buffs or debuffs. Auras have a duration that
  // is either finite or infinite. Auras have stacks.
  std::vector<Aura> active_auras_;

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
