// This file contains the main entry point for conducting simulation.

#include <algorithm>
#include <chrono>
#include <iostream>
#include <mutex>
#include <random>
#include <string>
#include <thread>

#include "absl/memory/memory.h"
#include "absl/strings/str_cat.h"
#include "glog/logging.h"

#include "simulator/engine.h"
#include "simulator/online_statistics.h"

#include "proto/simulation.pb.h"

namespace policygen {
Engine::Engine(const int num_threads) {
  LOG(INFO) << "Initialized Engine with " << num_threads << " threads.";
  pool_ = absl::make_unique<ThreadPool>(num_threads);
}

Engine::~Engine() { LOG(INFO) << "Destroying engine."; }

// Runs a small batch of iterations of the simulation.
// Adds the DPS values to the online statistics tracker.
// Iterations value should be chosen such that it is faster to run this in a
// single thread than it is to parallelize.
void MultipleIterationSimulation(const simulatorproto::SimulationConfig& config,
                                 const int iterations,
                                 OnlineStatistics* os) {
  std::random_device r;
  std::default_random_engine e(r());
  std::normal_distribution<double> normal_dist(20000.0, 1200.0);
  for (int i = 0; i < iterations; ++i) {
    os->AddValue(normal_dist(e));
  }
}

simulatorproto::SimulationResult Engine::Simulate(
    const simulatorproto::SimulationConfig& config) const {
  // Track our DPS distribution as it evolves with OnlineStatistics.
  // Do *at least* kMinIterations, and *at most* mKaxIterations.
  // If our CoefficientOfVariation (stderr / mean) is less than the target error
  // at any point, halt simulation and report the distribution.

  OnlineStatistics os;
  constexpr int kMinIterations = 100;
  const int kMaxIterations = config.max_iterations();
  const double kTargetError = config.target_error();

  MultipleIterationSimulation(config, kMinIterations, &os);

  // While we're above the target error and below the max iterations, insert a
  // new iteration into the queue and let the threads handle it.
  //
  /*
  while (os.Count() < kMaxIterations &&
         os.CoefficientOfVariation() < kTargetError) {
    auto future = pool_->Enqueue(SingleIterationSimulation, config, &os);
    future.get();
  }*/

  // Report the final result.
  simulatorproto::Distribution dps_distribution;
  dps_distribution.set_n(os.Count());
  dps_distribution.set_mean(os.Mean());
  dps_distribution.set_stddev(std::sqrt(os.Variance()));

  simulatorproto::SimulationResult r;
  r.mutable_dps_distribution()->MergeFrom(dps_distribution);
  r.set_metadata(absl::StrCat("Iterations: ", os.Count()));
  return r;
}
}  // namespace policygen
