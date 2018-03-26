#include <algorithm>
#include <chrono>
#include <functional>
#include <future>
#include <iostream>
#include <mutex>
#include <string>

#include "absl/strings/str_cat.h"
#include "glog/logging.h"

#include "simulator/core/config_summary.h"
#include "simulator/engine.h"
#include "simulator/simulate.h"
#include "simulator/util/online_statistics.h"

#include "proto/simulation.pb.h"

namespace simulator {

Engine::Engine(const int num_threads) : num_threads_(num_threads) {
  LOG(INFO) << "Initializing Engine with " << num_threads << " threads.";
}

// This main method spins until statistics tracker says we're done.
// We're done if
// a) we hit kMaxIterations, or
// b) we're above the kMinIterations and StdErr / Mean < target_error.
simulatorproto::SimulationResult Engine::Simulate(
    const simulatorproto::SimulationConfig& config_proto,
    const bool debug) const {
  // Handle debug single iteration case.
  if (debug) {
    LOG(INFO) << "Debug simulation requested. Single iteration, single thread.";
    const simulator::core::ConfigSummary config(config_proto);
    SimulationThread thread(config);
    double dps = thread.RunSingleIteration();
    simulatorproto::Distribution dps_distribution;
    dps_distribution.set_n(1);
    dps_distribution.set_mean(dps);
    dps_distribution.set_stddev(0);
    simulatorproto::SimulationResult r;
    r.mutable_dps_distribution()->MergeFrom(dps_distribution);
    r.set_metadata("DEBUG RESULT, SINGLE ITERATION");
    return r;
  }

  // Read in simulation termination conditions from config.
  const int kMinIterations = config_proto.min_iterations();
  const int kMaxIterations = config_proto.max_iterations();
  const double kTargetError = config_proto.target_error();

  // Parse configuration file into a configuration summary.
  const simulator::core::ConfigSummary config(config_proto);

  // Setting `cancel` to true forces tasks to finish; we use to early terminate.
  std::atomic_bool cancel(false);

  // Keep track of simulation-wide damage statistics here.
  simulator::util::OnlineStatistics metrics;

  // Compute iterations per thread. Ensure we actually do enough work to finish.
  const int iters = (kMaxIterations / num_threads_) + 1;

  // While the global cancel flag isn't set yet, run a `num_threads` async tasks
  // worth of work, each attempting to do `iters` simulation episodes.
  // clang-format off
  std::vector<std::future<void>> futures;
  for (int i = 0; i < num_threads_; ++i) {
    futures.emplace_back(
      std::async(std::launch::async, [iters, &config, &cancel, &metrics]() {
        for (int j = 0; j < iters; ++j) {
          if (cancel) break;
          const double dps = SimulationThread(config).RunSingleIteration();
          metrics.AddValue(dps);
        }
      })
    );
  }
  // clang-format on

  // Start timing our simulation.
  auto start = std::chrono::high_resolution_clock::now();

  // Computations are now running. Handle termination conditions here.
  while (metrics.Count() < kMaxIterations) {
    if (metrics.Count() > kMinIterations &&
        metrics.NormalizedError() < kTargetError) {
      LOG(INFO) << "Breaking early: target error hit.";
      LOG(INFO) << "Normalized error: " << metrics.NormalizedError();
      cancel = true;
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
  auto tput = static_cast<double>(metrics.Count()) / t.count();
  std::string metadata_string(absl::StrCat("Throughput: ", tput, " iters/us"));

  // Report the final result.
  simulatorproto::Distribution dps_distribution;
  dps_distribution.set_n(metrics.Count());
  dps_distribution.set_mean(metrics.Mean());
  dps_distribution.set_stddev(std::sqrt(metrics.Variance()));
  simulatorproto::SimulationResult r;
  r.mutable_dps_distribution()->MergeFrom(dps_distribution);
  r.set_metadata(metadata_string);
  return r;
}
}  // namespace simulator
