#include <atomic>
#include <random>

#include "simulator/core/config_processor.h"
#include "simulator/core/player.h"
#include "simulator/core/simulation_state.h"

#include "simulator/util/online_statistics.h"
#include "simulator/util/rng.h"
#include "simulator/util/timer_wheel.h"

#include "simulator/simulate.h"

using simulator::core::EncounterSummary;
using simulator::core::EquipmentSummary;
using simulator::core::Player;
using simulator::core::PolicyFunctor;
using simulator::core::SimulationState;

using simulator::util::OnlineStatistics;
using simulator::util::RNG;
using simulator::util::TimerWheel;

using std::chrono::milliseconds;

namespace simulator {

void RunBatch(const EncounterSummary& encounter,
              const EquipmentSummary& equipment, const PolicyFunctor& policy,
              const int num_iterations,
              const std::atomic_bool& cancellation_token,
              OnlineStatistics* damage_tracker) {
  int current_iteration = 0;
  while (!cancellation_token && current_iteration < num_iterations) {
    damage_tracker->AddValue(RunSingleIteration(encounter, equipment, policy));
    ++current_iteration;
  }
}

double RunSingleIteration(const EncounterSummary& encounter,
                          const EquipmentSummary& equipment,
                          const PolicyFunctor& policy) {
  RNG rng;
  SimulationState sim_state;
  TimerWheel event_manager;
  double damage_sum = 0;

  // Set up encounter time parameters.
  const double time_lb = (1 - encounter.time_variance) * encounter.time_target;
  const double time_ub = (1 + encounter.time_variance) * encounter.time_target;
  const milliseconds combat_time_maximum =
      milliseconds((int) rng.UniformDouble(time_lb, time_ub));

  // Set up fixed, known-time encounter events (spawn, bloodlust, ...)
  for (const auto& raid_event : encounter.raid_events) {
    TimerEvent e(raid_event.GetCallback());
    event_manager.Schedule(&e, raid_event.GetScheduledTime().count());
  }

  sim_state.combat_time_elapsed = std::chrono::milliseconds::zero();
  sim_state.combat_potion_used = false;

  while (sim_state.combat_time_elapsed < combat_time_maximum) {
    const auto ticks = event_manager.TicksUntilNextEvent();
    sim_state.combat_time_elapsed += milliseconds(ticks);
    event_manager.Advance(ticks);

    // Player makes an action choice somewhere in here.
    // Simulation automatically schedules new bits and pieces here
  }
  return damage_sum;
}

}  // namespace simulator
