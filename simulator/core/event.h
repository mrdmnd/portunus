#pragma once

#include <chrono>
#include <functional>

#include "simulator/core/simulation_state.h"

using std::chrono::milliseconds;

namespace simulator {
namespace core {

// A RaidEvent contains a callback and a scheduled time to fire that callback.
// Our callbacks get a pointer to simulation state, and can modify it however
// they want.
class Event {
 public:
  Event(const milliseconds scheduled_time,
        const std::function<void(SimulationState*)> callback) :
    scheduled_time_(scheduled_time),
    callback_(callback){};

  inline milliseconds GetScheduledTime() const { return scheduled_time_; }
  inline std::function<void(SimulationState*)> GetCallback() const {
    return callback_;
  }

 private:
  const milliseconds scheduled_time_;
  const std::function<void(SimulationState*)> callback_;
};
}  // namespace core
}  // namespace simulator
