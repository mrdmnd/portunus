#pragma once
#include "proto/simulation.pb.h"

using namespace simulate;

// Takes as input a configuration option, and returns a result.
class Engine {
 public:
  Engine() = delete;
  Engine(const int num_threads) : num_threads_(num_threads){};
  ~Engine();

  // Simulate the input simulation configuration on all threads.
  SimulationResult Simulate(SimulationConfig config) const;

  // Disallow {move,copy} {construction,assignment}.
  Engine(const Engine& other) = delete;
  Engine(const Engine&& other) = delete;
  Engine& operator=(const Engine& other) = delete;
  Engine& operator=(const Engine&& other) = delete;

 private:
  // Class members.
  int num_threads_;
};
