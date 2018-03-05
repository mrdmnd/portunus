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

#include "simulator/engine.h"
#include "simulator/online_statistics.h"
#include "simulator/simulate.h"

#include "proto/simulation.pb.h"

namespace {
// Run up to `num_iterations` episodes of the simulation.
// Adds the DPS values to the online statistics tracker `os` as we go.
void BatchSimulation(const simulatorproto::SimulationConfig& config,
                     const int num_iterations,
                     const std::atomic_bool& cancelled,
                     policygen::OnlineStatistics* os) {
  int current_iteration = 0;
  while (!cancelled && current_iteration < num_iterations) {
    os->AddValue(policygen::SingleIteration(config));
    ++current_iteration;
  }
}
}  // namespace

namespace policygen {
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
  // Read in simulation bounds from config file.
  constexpr int kMinIterations = 100;  // TODO(mrdmnd) - don't hardcode?
  const int kMaxIterations = config.max_iterations();
  const double kTargetError = config.target_error();

  // Parse configuration file into post-processed bits.
  // We'll pass copies of these into the BatchSimulation function,
  // which in turn will pass copies in to each single iteration.
  // The iterations will generate a random seed and use that for their
  // target time.
  // const Encounter encounter = ParseEncounterFromSimulationConfig(config);
  // const Policy policy = ParsePolicyFromSimulationConfig(config);
  // const Player player = ParsePlayerFromSimulationConfig(config);

  // Setting cancellation_token = true forces BatchSimulation tasks to finish.
  std::atomic_bool cancellation_token(false);

  // Keep track of simulation-wide running statistics here.
  OnlineStatistics os;

  // Compute iterations per thread. Ensure we actually do enough work to finish.
  const int ipt = (kMaxIterations / pool_->NumThreads()) + 1;

  // Store handles to our workers.
  std::vector<std::future<void>> futures;

  // Use our threadpool to enqueue the BatchSimulation function.
  // It takes the config *by COPY* despite being the fn signature looking like a
  // reference parameter; this is okay with us, this object is small, and we
  // don't change it. This might be worth revisiting at some point.
  for (int i = 0; i < pool_->NumThreads(); ++i) {
    futures.emplace_back(pool_->Enqueue(
        BatchSimulation, config, ipt, std::ref(cancellation_token), &os));
  }

  // Start timing our simulation.
  auto start = std::chrono::high_resolution_clock::now();

  // Computations are now running. Break when done.
  while (os.Count() < kMaxIterations) {
    if (os.Count() > kMinIterations &&
        os.StdError() / os.Mean() < kTargetError) {
      LOG(INFO) << "Breaking early: target error hit: "
                << os.CoefficientOfVariation();
      cancellation_token = true;
      break;
    }
  }

  // Finish timing.
  auto stop = std::chrono::high_resolution_clock::now();

  // Force the threads to rejoin us.
  for (auto& future : futures) {
    future.get();
  }

  // Get some metadata.
  auto t = std::chrono::duration_cast<std::chrono::microseconds>(stop - start);
  std::string metadata_string(
      absl::StrCat("Throughput: ",
                   static_cast<double>(os.Count()) / t.count(),
                   "iters/us, ",
                   "Simulation time: ",
                   t.count(),
                   "us"));

  // Report the final result.
  simulatorproto::Distribution dps_distribution;
  dps_distribution.set_n(os.Count());
  dps_distribution.set_mean(os.Mean());
  dps_distribution.set_stddev(std::sqrt(os.Variance()));

  simulatorproto::SimulationResult r;
  r.mutable_dps_distribution()->MergeFrom(dps_distribution);
  r.set_metadata(metadata_string);
  return r;
}  // namespace policygen
}  // namespace policygen
