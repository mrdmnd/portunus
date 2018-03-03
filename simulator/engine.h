#pragma once
#include "proto/simulation.pb.h"

namespace policygen {
// Engine class is meant to be a "long-running" class that can execute many
// simulations, one-after-the-other. It keeps a long-lived thread pool alive.
// Each call to Simulate sets up a new work queue that the threads can pull
// from, as well as a new statics-tracking object for reporting + early
// termination.

// Takes as input a configuration option, and returns a result.
class Engine {
 public:
  Engine(const int num_threads);
  Engine();
  ~Engine();

  // Simulate the input simulation configuration on all threads.
  simulatorproto::SimulationResult Simulate(
      const simulatorproto::SimulationConfig& conf) const;

  // Disallow {move,copy} {construction,assignment}.
  Engine(const Engine& other) = delete;
  Engine(const Engine&& other) = delete;
  Engine& operator=(const Engine& other) = delete;
  Engine& operator=(const Engine&& other) = delete;

 private:
  // Class members.
  const int num_threads_;
};
}  // namespace policygen
