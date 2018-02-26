#include "proto/simulation.pb.h"

using namespace simulate;

// Takes as input a configuration option, and returns a result.
class Engine {
public:
  Engine() = default;
  ~Engine() = default;
  Engine(const int num_threads) : num_threads_(num_threads){};
  SimulationResult Simulate(SimulationConfig config) const;

private:
  // Disallow copy construction and assignment.
  Engine(const Engine &other) = delete;
  Engine &operator=(const Engine &other) = delete;

  // Disallow move construction and assignment.
  Engine(const Engine &&other) = delete;
  Engine &operator=(const Engine &&other) = delete;

  // Class members.
  int num_threads_;
};
