#include <atomic>
#include <chrono>
#include <random>

#include "simulator/core/config_summary.h"
#include "simulator/core/player.h"
#include "simulator/core/simulation_state.h"
#include "simulator/util/online_statistics.h"
#include "simulator/util/rng.h"
#include "simulator/util/timer_wheel.h"

#include "simulator/simulate.h"

using simulator::core::ConfigSummary;
using simulator::core::Player;
using simulator::core::SimulationState;
using simulator::core::enums::EventTag;

using simulator::util::OnlineStatistics;
using simulator::util::RNG;
using simulator::util::TimerEvent;
using simulator::util::TimerWheel;

using std::chrono::milliseconds;

namespace simulator {

void RunBatch(const ConfigSummary& config, const int num_iterations,
              const std::atomic_bool& cancellation_token,
              OnlineStatistics* damage_tracker) {
  int current_iteration = 0;
  while (!cancellation_token && current_iteration < num_iterations) {
    damage_tracker->AddValue(RunSingleIteration(config));
    ++current_iteration;
  }
}

double RunSingleIteration(const ConfigSummary& config) {
  RNG rng;
  SimulationState sim_state;
  TimerWheel event_manager;
  double damage_sum = 0;

  // Set up encounter constant parameters
  const PolicyInterface policy = config.GetPolicy();
  const milliseconds time_lb = config.GetTimeMin();
  const milliseconds time_ub = config.GetTimeMax();
  const milliseconds combat_end_timestamp = rng.Uniform(time_lb, time_ub);

  // Set up fixed, known-time encounter events (spawn, bloodlust, ...)
  // Load these into the event manager.
  for (const auto& raid_event : config.GetRaidEvents()) {
    // Immediately execute any spawn events, or events with scheduled_time == 0
    // Don't bother scheduling them, just fire them off directly.
    if (raid_event.GetTag() == EventTag::ENEMY_SPAWN ||
        raid_event.GetScheduledTime() == std::chrono::milliseconds::zero()) {
      raid_event.GetCallback()(&sim_state);
    } else {
      const auto cb = raid_event.GetCallback();
      TimerEvent<std::function<void(SimulationState*)>, SimulationState*> e(
          {cb, &sim_state});
      event_manager.Schedule(&e, raid_event.GetScheduledTime().count());
    }
  }

  sim_state.combat_time_elapsed = std::chrono::milliseconds::zero();
  sim_state.combat_potion_used = false;

  while (sim_state.combat_time_elapsed < combat_end_timestamp) {
    event_manager.Advance(event_manager.TicksUntilNextEvent());
    const Action action = policy.Evaluate(sim_state);
  }
  return damage_sum;
}
}  // namespace simulator
