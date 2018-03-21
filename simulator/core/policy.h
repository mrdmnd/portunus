#pragma once

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
  // I'm not sure exactly what type I need to initialize a policy yet. TBD.
  PolicyInterface() = default;
  explicit PolicyInterface(const int placeholder);

  // Disable move + copy constructors and assignments.
  PolicyInterface(const PolicyInterface& other) = delete;
  PolicyInterface(PolicyInterface&& other) = delete;
  PolicyInterface& operator=(const PolicyInterface& other) = delete;
  PolicyInterface& operator=(PolicyInterface&& other) = delete;

  // Policies have a way to evaluate a simulation state and generate an action.
  virtual Action Evaluate(const SimulationState& state) const = 0;
};

}  // namespace core
}  // namespace simulator
