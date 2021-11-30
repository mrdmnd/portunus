#pragma once

#include <chrono>
#include <random>

#include "glog/logging.h"

namespace simulator {
namespace util {

using std::chrono::milliseconds;

class RngEngine {
 public:
  RngEngine() :
    uniform_dist_(std::uniform_real_distribution<double>(0.0, 1.0)){};

  // A (very slightly) optimized call that re-uses a previous dist instance.
  inline bool Roll(double chance) {
    DCHECK(chance >= 0) << "RNG Roll Chance < 0.";
    DCHECK(chance <= 1) << "RNG Roll Chance > 1.";
    return uniform_dist_(engine_) < chance;
  }

  // Returns an integer in [min, max].
  inline int Uniform(int min, int max) {
    std::uniform_int_distribution<int> dist(min, max);
    return dist(engine_);
  }

  // Returns a `milliseconds` duration in [min, max]
  inline milliseconds Uniform(milliseconds min, milliseconds max) {
    std::uniform_int_distribution<milliseconds::rep> dist(min.count(),
                                                          max.count());
    return milliseconds(dist(engine_));
  }

  // Returns a double in [min, max)
  inline double Uniform(double min, double max) {
    std::uniform_real_distribution<double> dist(min, max);
    return dist(engine_);
  }

  // Returns a double selected from a normal distribution.
  // If truncate_low = true, we return a double in [0, inf)
  inline double Normal(double mean, double stddev, bool truncate_low = false) {
    std::normal_distribution<double> dist(mean, stddev);
    const double value = dist(engine_);
    return (value < 0.0 && truncate_low) ? 0.0 : value;
  }

 private:
  // Our core randomness generator.
  // Can experiment with Linear Congruential or Mersenne Twisters.
  std::default_random_engine engine_;

  // A uniform distribution instance that we can re-use for the Roll() call.
  std::uniform_real_distribution<double> uniform_dist_;
};
}  // namespace util
}  // namespace simulator
