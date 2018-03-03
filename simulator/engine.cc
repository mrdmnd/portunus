// This file contains the main entry point for conducting simulation.

#include <algorithm>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>

#include "absl/memory/memory.h"
#include "glog/logging.h"
#include "simulator/engine.h"

#include "proto/simulation.pb.h"

namespace policygen {
Engine::Engine(const int num_threads) {
  LOG(INFO) << "Initialized Engine with " << num_threads << " threads.";
  pool_ = absl::make_unique<ThreadPool>(num_threads);
}

Engine::~Engine() { LOG(INFO) << "Destroying engine."; }

simulatorproto::SimulationResult Engine::Simulate(
    const simulatorproto::SimulationConfig& config) const {
  simulatorproto::Distribution dps_distribution;
  dps_distribution.set_mean(100.0);
  dps_distribution.set_variance(10.0);

  simulatorproto::SimulationResult r;
  r.mutable_dps_distribution()->MergeFrom(dps_distribution);
  r.set_metadata("Hello world from engine.cc!");
  return r;
}
}  // namespace policygen
