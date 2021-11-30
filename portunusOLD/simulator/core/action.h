#pragma once

// An *action* is a lightweight class that represents a player input.
// Sample actions include
// Wait - a default action, the no-op input
// QueueCast - set up a new spell cast in the cast queue. If you QueueCast a and
// then QueueCast b, you just overwrite the spell queue with b.
// InterruptCast - cancel the currently casting spell

namespace simulator {
namespace core {
class Action {};
class Wait : public Action {};
class QueueCast : public Action {
 public:
  explicit QueueCast(const int spell_id) : spell_id_(spell_id) {}

 private:
  const int spell_id_;
};
}  // namespace core
}  // namespace simulator
