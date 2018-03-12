#pragma once

#include <map>

namespace simulator {
namespace util {

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
}  // namespace util
}  // namespace simulator
