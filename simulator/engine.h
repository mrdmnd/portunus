#pragma once

#include "simulator/util/threadpool.h"

#include "proto/simulation.pb.h"

namespace simulator {
// The `Engine` class is meant to be long-live, executing lots of simulations
// one-after-the-other. It keeps a long-lived thread pool alive to mitigate the
// overhead of launching new threads each time we want to do work.
class Engine {
 public:
  Engine() = delete;
  Engine(const int num_threads);

  // Simulate the input simulation configuration on all threads.
  simulatorproto::SimulationResult Simulate(
      const simulatorproto::SimulationConfig& conf_proto,
      const bool debug) const;

  // Disallow {move,copy} {construction,assignment}.
  Engine(const Engine& other) = delete;
  Engine(const Engine&& other) = delete;
  Engine& operator=(const Engine& other) = delete;
  Engine& operator=(const Engine&& other) = delete;

 private:
  std::unique_ptr<simulator::util::ThreadPool> pool_;
};
}  // namespace simulator
