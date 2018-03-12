#pragma once

#include <chrono>
#include <functional>

#include "simulator/core/simulation_state.h"

#include "simulator/core/constants.h"

using std::chrono::milliseconds;

namespace simulator {
namespace core {

// A RaidEvent contains a callback and a scheduled time to fire that callback.
// Our callbacks get a pointer to simulation state, and can modify it however
// they want.
class Event {
 public:
  Event(const milliseconds scheduled_time,
        const std::function<void(SimulationState*)> callback,
        const simulator::core::enums::EventTag tag) :
    scheduled_time_(scheduled_time),
    callback_(callback),
    tag_(tag){};

  inline milliseconds GetScheduledTime() const { return scheduled_time_; }
  inline std::function<void(SimulationState*)> GetCallback() const {
    return callback_;
  }
  inline simulator::core::enums::EventTag GetTag() const { return tag_; }

 private:
  const milliseconds scheduled_time_;
  const std::function<void(SimulationState*)> callback_;
  const simulator::core::enums::EventTag tag_;
};
}  // namespace core
}  // namespace simulator
