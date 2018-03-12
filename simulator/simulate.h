#pragma once

#include <atomic>

#include "simulator/core/config_summary.h"
#include "simulator/util/online_statistics.h"

namespace simulator {

// The entry point and main logic for doing a small batch of simulations.
// We assume that we have already processed a SimulationConfig proto into a
// ConfigSummary object.

// Executes asynchronously in `engine.cc`, is signalled to halt early with the
// cancellation token if our main thread detects an early stopping condition is
// met (happens when target_error threshold is met).
void RunBatch(const simulator::core::ConfigSummary& config,
              const int num_iterations,
              const std::atomic_bool& cancellation_token,
              simulator::util::OnlineStatistics* damage_tracker);

// Performs a single, simple, synchronous simulation iteration.
// Returns the DPS value from this iteration.
double RunSingleIteration(const simulator::core::ConfigSummary& config);
}  // namespace simulator
