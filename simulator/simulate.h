#pragma once

#include <atomic>
#include <chrono>

#include "simulator/core/config_summary.h"
#include "simulator/core/damage_log.h"
#include "simulator/core/policy.h"
#include "simulator/core/simulation_state.h"
#include "simulator/util/online_statistics.h"
#include "simulator/util/rng.h"
#include "simulator/util/timer_wheel.h"

namespace simulator {

// The entry point and main logic for doing a small batch of simulations.
// We assume that we have already processed a SimulationConfig proto into a
// ConfigSummary object.

// Executes asynchronously in `engine.cc`, is signalled to halt early with the
// cancellation token if our main thread detects an early stopping condition is
// met (happens when target_error threshold is met).
void RunBatch(const core::ConfigSummary& config,
              const int num_iterations,
              const std::atomic_bool& cancellation_token,
              util::OnlineStatistics* damage_tracker);

// This is the wrapper class that keeps track of a single episode of simulation.
class SimulationContext {
 public:
  SimulationContext() = delete;
  explicit SimulationContext(const core::ConfigSummary& config);

  // Disallow {move,copy} {construction,assignment}.
  SimulationContext(const SimulationContext& other) = delete;
  SimulationContext(const SimulationContext&& other) = delete;
  SimulationContext& operator=(const SimulationContext& other) = delete;
  SimulationContext& operator=(const SimulationContext&& other) = delete;

  // Performs a single, simple, synchronous simulation iteration.
  // Returns the DPS value from this iteration.
  double RunSingleIteration();  // NOT CONST, mutates members.

 private:
  void InitPlayer(const core::CombatStats& gear_stats,
                  const std::vector<core::Spell>& gear_effects,
                  const std::vector<core::Talent>& talents);
  void InitRaidEvents(const std::vector<core::Event>& raid_events);

  core::PolicyInterface* policy_;
  std::chrono::milliseconds combat_length_;
  util::RngEngine rng_;
  util::TimerWheel event_manager_;
  core::SimulationState state_;
  core::DamageLog damage_log_;
};
}  // namespace simulator
