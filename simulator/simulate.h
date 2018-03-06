#pragma once

#include <atomic>

#include "simulator/util/config_processors.h"
#include "simulator/util/online_statistics.h"

using simulator::util::EncounterSummary;
using simulator::util::EquipmentSummary;
using simulator::util::OnlineStatistics;
using simulator::util::PolicyFunctor;

namespace simulator {

// The entry point and main logic for doing a small batch of simulations.
// We assume that we have already processed a SimulationConfig object into an
// EncounterSummary, EquipmentSummary, and Policy function.

// Executes asynchronously in `engine.cc`, is signalled to halt early with the
// cancellation token if our main thread detects an early stopping condition is
// met (happens when target_error threshold is met).
void RunBatch(const EncounterSummary& encounter,
              const EquipmentSummary& equipment,
              const PolicyFunctor& policy,
              const int num_iterations,
              const std::atomic_bool& cancellation_token,
              OnlineStatistics* damage_tracker);

// Performs a single, simple, synchronous simulation iteration.
// Returns the DPS value from this iteration.
double RunSingleIteration(const EncounterSummary& encounter,
                          const EquipmentSummary& equipment,
                          const PolicyFunctor& policy);
}  // namespace simulator
