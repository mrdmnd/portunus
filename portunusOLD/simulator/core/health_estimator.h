#pragma once

#include <map>

#include "glog/logging.h"

namespace {

// Interpolates a key into a (key, val) map.
// If key < min_val) then return min_val).
// If key > max_val) then return max_val).
// If key is in the map, return it directly.
// If key is inbetween some map keys, interpolate between them.
double MapInterpolate(const double key, const std::map<double, double>& map) {
  const auto ub = map.upper_bound(key);
  if (ub == map.end()) {
    return std::prev(ub)->second;
  }
  if (ub == map.begin()) {
    return ub->second;
  }
  const auto lb = std::prev(ub);
  const auto a = (key - lb->first) / (ub->first - lb->first);
  return (a) * (ub->second) + (1 - a) * (lb->second);
}
}  // namespace

namespace simulator {
namespace core {

// A class that supports arbitrary pre-determined enemy health scheduling.
class HealthEstimator {
 public:
  // The public interface for this class is a method that can be called to get
  // the current health percentage of this enemy, given the time progression
  // through the overall encounter. We implement this as linear interpolation
  // between (construction-time) specified control points.
  HealthEstimator() = default;
  explicit HealthEstimator(const std::map<double, double>& control_points) :
    control_points_(control_points){};

  ~HealthEstimator() = default;
  HealthEstimator(const HealthEstimator& other) = default;
  HealthEstimator& operator=(const HealthEstimator& other) = default;

  // Encounter progress is measured between 0.0 and 1.0, and can be computed as
  // current_time / total_combat_length in calling code.
  inline double InterpolateHealthPercentage(const double time_progress) {
    DCHECK(time_progress >= 0.0);
    DCHECK(time_progress <= 1.0);
    return MapInterpolate(time_progress, control_points_);
  }

  // Convenience functions for some common preconfigured estimators.
  static HealthEstimator UniformHealthEstimator() {
    return HealthEstimator({{0.0, 1.0}, {1.0, 0.0}});
  }

  static HealthEstimator BurstHealthEstimator() {
    return HealthEstimator(
        {{0.0, 1.0}, {0.05, 0.90}, {0.10, 0.85}, {1.0, 0.0}});
  }

  static HealthEstimator ExecuteHealthEstimator() {
    return HealthEstimator({{0.0, 1.0}, {0.80, 0.30}, {1.0, 0.0}});
  }

  static HealthEstimator BurstExecuteHealthEstimator() {
    return HealthEstimator(
        {{0.0, 1.0}, {0.05, 0.90}, {0.10, 0.85}, {0.80, 0.30}, {1.0, 0.0}});
  }

 private:
  // Control_points is a map from time_progress to health_progress values, i.e.
  // a uniform estimator has values that look like this:
  // { { 0.0, 1.0 }, { 0.05, 0.95}, ... {0.95, 0.05}, {1.0, 0.0} }
  const std::map<double, double> control_points_;
};
}  // namespace core
}  // namespace simulator
