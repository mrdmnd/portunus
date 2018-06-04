#pragma once

#include "proto/service.pb.h"

namespace simulator {
class Engine {
 public:
  Engine() = delete;
  Engine(const int num_threads);

  // Simulate the input simulation configuration on all threads.
  simulatorproto::SimulationResult Simulate(
      const simulatorproto::SimulationConfig& simulation_conf) const;

  // Disallow {move,copy} {construction,assignment}.
  Engine(const Engine& other) = delete;
  Engine(const Engine&& other) = delete;
  Engine& operator=(const Engine& other) = delete;
  Engine& operator=(const Engine&& other) = delete;

 private:
  const int num_threads_;
};
}  // namespace simulator
