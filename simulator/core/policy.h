#pragma once

#include "simulator/core/simulation_state.h"
#include "simulator/core/spell.h"

namespace simulator {
namespace core {

// A PolicyInterface is an interface with two intended implementations:
// a) The standard, SIMC-style "priority list" which explicitly selects the
// first action available in an ordered tree.
// b) A neural network which is evaluated every time we need to select
// an action, which provides a probability distribution over all possible player
// actions. The policy selects the action with the highest probability that is
// not illegal to perform in this simulation state.
class PolicyInterface {
 public:
  PolicyInterface() = default;
  virtual ~PolicyInterface() = default;

  // Policies have a way to evaluate a simulation state and generate an action.
  virtual Spell Evaluate(const SimulationState& state) const = 0;
};

class DeterministicPolicy : public PolicyInterface {
 public:
  DeterministicPolicy(const int placeholder) : placeholder_(placeholder) {}
  virtual ~DeterministicPolicy() = default;

  Spell Evaluate(const SimulationState& state) const override {
    Spell s;
    s.cooldown = std::chrono::milliseconds(placeholder_);
    return s;
  }

 private:
  const int placeholder_;
};
}  // namespace core
}  // namespace simulator
