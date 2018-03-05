#include <random>

#include "proto/simulation.pb.h"

namespace policygen {
double SingleIteration(const simulatorproto::SimulationConfig& config) {
  std::random_device r;
  std::default_random_engine e(r());
  std::normal_distribution<double> normal_dist(20000.0, 190.0);
  return normal_dist(e);
  // Choose a simulation timelength that is consistent with the config.
}
}  // namespace policygen
