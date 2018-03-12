#pragma once

#include <functional>

#include "simulator/core/simulation_state.h"

#include "simulator/util/timer_wheel.h"

using simulator::core::SimulationState;
using simulator::util::TimerEvent;
using std::chrono::milliseconds;

namespace simulator {
namespace core {

// Our raid events modify SimulationState, and return nothing.
using CBType = std::function<void(SimulationState*)>;

// A RaidEvent contains a callback and a scheduled time to fire that callback.
class RaidEvent {
 public:
  RaidEvent(const milliseconds scheduled_time, const CBType callback) :
    scheduled_time_(scheduled_time),
    callback_(callback){};

  inline milliseconds GetScheduledTime() const { return scheduled_time_; }
  inline CBType GetCallback() const { return callback_; }

 private:
  const milliseconds scheduled_time_;
  const CBType callback_;
};
}  // namespace core
}  // namespace simulator
