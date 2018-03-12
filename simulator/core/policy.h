#pragma once

namespace simulator {
namespace core {

// A GenericPolicy is an abstract superclass with two intended implementations:
// a) The standard, SIMC-style "priority list" which explicitly selects the
// first action available in the (ordered) list or tree.
// OR
// b) A new-style neural network which is evaluated every time we need to select
// an action, which provides a probability distribution over all possible player
// actions. The policy selects the action with the highest probability that is
// not illegal to perform in this simulation state.
class GenericPolicy {
  // I'm not sure exactly what type I need to initialize a policy yet. TBD.
  GenericPolicy() = delete;
  explicit GenericPolicy(const int placeholder);

  // Disable move + copy constructors and assignments.
  GenericPolicy(const GenericPolicy& other) = delete;
  GenericPolicy(GenericPolicy&& other) = delete;
  GenericPolicy& operator=(const GenericPolicy& other) = delete;
  GenericPolicy& operator=(GenericPolicy&& other) = delete;

  // Policies have a way to evaluate a simulation state and generate an action.
  virtual Action Evaluate(const SimulationState& state) = 0;
};

}  // namespace core
}  // namespace simulator
