// This file contains the main entry point for conducting simulation.

#include <algorithm>
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

// Runs a single iteration of the simulation specified in the config file.
// Adds the DPS value to the online statistics tracker.
void SingleIterationSimulation(const simulatorproto::SimulationConfig& config,
                               OnlineStatistics* os) {
  std::random_device r;
  std::default_random_engine e(r());
  std::normal_distribution<double> normal_dist(20000.0, 1200.0);
  const double sample = normal_dist(e);
  os->AddValue(sample);
}

simulatorproto::SimulationResult Engine::Simulate(
    const simulatorproto::SimulationConfig& config) const {
  // Create an online statistics tracker to keep track of our DPS distribution.
  OnlineStatistics os;
  constexpr int kMinIterations = 100;
  const int kMaxIterations = config.max_iterations();
  const double kTargetError = config.target_error();
  while (os.Count() < kMinIterations) {
    auto future = pool_->enqueue(SingleIterationSimulation, config, &os);
    future.get();
  }

  // While we're above the target error and below the max iterations,
  // insert a new iteration into the queue and let the threads handle it.
  // while (os.Count() < max_iterations &&
  //       os.StdError() / os.Mean() < target_error) {
  // Might also want to check that the pool is empty. If the pool is not
  // empty, but we submit a bunch more work *already satisfying* these
  // conditions, then we're doing unnecessary work.
  // auto future = pool_->enqueue(SingleIterationSimulation, config, &os);
  // future.get();
  //}

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
