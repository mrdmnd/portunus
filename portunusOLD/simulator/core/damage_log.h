#pragma once

#include <chrono>
#include <numeric>
#include <vector>

#include "simulator/core/actor.h"

namespace simulator {
namespace core {

// A tiny bookkeeping class for tracking damage events to various raid targets.
// TODO(mrdmnd) - ensure there are no lifetime issues with these events living
// longer than the actors they reference.
struct DamageEvent {
  DamageEvent(double damage, const Actor& target,
              const std::chrono::milliseconds timestamp) :
    damage(damage),
    target(target),
    timestamp(timestamp) {}
  const double damage;
  const Actor& target;
  const std::chrono::milliseconds timestamp;
};

class DamageLog {
 public:
  DamageLog() = default;
  void Clear() { damage_events_.clear(); }
  void AddDamageEvent(const DamageEvent& event) {
    damage_events_.push_back(event);
  }
  double DamagePerSecond() const {
    return 1000.0 * TotalDamage() / CombatLength().count();
  }

 private:
  // Assumption: vector is ordered by timestamp.
  std::vector<DamageEvent> damage_events_;

  std::chrono::milliseconds CombatLength() const {
    return damage_events_.back().timestamp - damage_events_.front().timestamp;
  }
  double TotalDamage() const {
    return std::accumulate(damage_events_.begin(), damage_events_.end(), 0.0,
                           [](double accumulator, const DamageEvent& next) {
                             return accumulator + next.damage;
                           });
  }

  // Could implement this for priority target damage reporting...
  // double TotalDamageToTarget(const Actor& target) { return std:: }
};
}  // namespace core
}  // namespace simulator
