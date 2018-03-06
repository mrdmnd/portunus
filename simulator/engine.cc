// This file contains the main entry point for conducting simulation.

#include <algorithm>
#include <chrono>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>

#include "absl/memory/memory.h"
#include "absl/strings/str_cat.h"
#include "glog/logging.h"

#include "simulator/config_processors.h"
#include "simulator/engine.h"
#include "simulator/online_statistics.h"
#include "simulator/simulate.h"

#include "proto/simulation.pb.h"

namespace policygen {
using namespace configprocess;

Engine::Engine(const int num_threads) {
  LOG(INFO) << "Initialized Engine with " << num_threads << " threads.";
  pool_ = absl::make_unique<ThreadPool>(num_threads);
}

Engine::~Engine() { LOG(INFO) << "Destroying engine."; }

// This main method spins until statistics tracker says we're done.
// We're done if
// a) we hit kMaxIterations, or
// b) we're above the kMinIterations and StdErr / Mean < target_error.
simulatorproto::SimulationResult Engine::Simulate(
    const simulatorproto::SimulationConfig& config) const {
  // Read in simulation termination conditions from config.
  const int kMinIterations = config.min_iterations();
  const int kMaxIterations = config.max_iterations();
  const double kTargetError = config.target_error();
  // Parse configuration file into post-processed bits. We'll pass these into
  // the RunBatch function, which in turn will pass to every single iteration.
  const EncounterSummary encounter(config.encounter_config());
  const EquipmentSummary equipment(config.player_config().gearset());
  const PolicyFunctor policy(config.policy());

  // Setting `cancellation_token = true` forces BatchSimulation tasks to finish.
  std::atomic_bool cancellation_token(false);

  // Keep track of simulation-wide damage statistics here.
  OnlineStatistics metric_tracker;

  // Compute iterations per thread. Ensure we actually do enough work to finish.
  const int iters_per_thread = (kMaxIterations / pool_->NumThreads()) + 1;

  // Use our threadpool to enqueue the function RunBatch, from simulate.h.
  std::vector<std::future<void>> futures;
  for (size_t i = 0; i < pool_->NumThreads(); ++i) {
    futures.emplace_back(pool_->Enqueue(policygen::RunBatch,
                                        encounter,
                                        equipment,
                                        policy,
                                        iters_per_thread,
                                        std::ref(cancellation_token),
                                        &metric_tracker));
  }

  // Start timing our simulation.
  auto start = std::chrono::high_resolution_clock::now();

  // Computations are now running. Handle termination conditions here.
  while (metric_tracker.Count() < kMaxIterations) {
    if (metric_tracker.Count() > kMinIterations &&
        metric_tracker.NormalizedError() < kTargetError) {
      LOG(INFO) << "Breaking early: target error hit.";
      LOG(INFO) << "Normalized error: " << metric_tracker.NormalizedError();
      cancellation_token = true;
      break;
    }
  }

  // Finish timing.
  auto stop = std::chrono::high_resolution_clock::now();

  // Force the simulation threads to rejoin main thread.
  for (auto& future : futures) {
    future.get();
  }

  // Get some metadata on throughput / timing / whatever.
  auto t = std::chrono::duration_cast<std::chrono::microseconds>(stop - start);
  auto tput = static_cast<double>(metric_tracker.Count()) / t.count();
  std::string metadata_string(absl::StrCat("Throughput: ", tput, " iters/us"));

  // Report the final result.
  simulatorproto::Distribution dps_distribution;
  dps_distribution.set_n(metric_tracker.Count());
  dps_distribution.set_mean(metric_tracker.Mean());
  dps_distribution.set_stddev(std::sqrt(metric_tracker.Variance()));
  simulatorproto::SimulationResult r;
  r.mutable_dps_distribution()->MergeFrom(dps_distribution);
  r.set_metadata(metadata_string);
  return r;
}
}  // namespace policygen
