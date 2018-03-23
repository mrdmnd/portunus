#pragma once

#include "simulator/util/threadpool.h"

#include "proto/simulation.pb.h"

namespace simulator {
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
  const int num_threads_;
};
}  // namespace simulator
