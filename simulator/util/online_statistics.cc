#include <cmath>
#include <limits>
#include <mutex>

#include "simulator/util/online_statistics.h"

#define QUIET_NAN (std::numeric_limits<double>::quiet_NaN())

namespace simulator {
namespace util {
void OnlineStatistics::AddValue(double value) {
  std::lock_guard<std::mutex> guard(mutex_);
  n_++;
  double delta1 = value - moment1_;
  moment1_ += (delta1 / n_);
  double delta2 = value - moment1_;
  moment2_ += delta1 * delta2;
}

long OnlineStatistics::Count() const {
  std::lock_guard<std::mutex> guard(mutex_);
  return n_;
}

double OnlineStatistics::Mean() const {
  std::lock_guard<std::mutex> guard(mutex_);
  return moment1_;
}

double OnlineStatistics::Variance() const {
  std::lock_guard<std::mutex> guard(mutex_);
  return (n_ > 1) ? moment2_ / (n_ - 1) : QUIET_NAN;
}

// SQRT(variance / n)
double OnlineStatistics::StdError() const {
  std::lock_guard<std::mutex> guard(mutex_);
  return (n_ > 1) ? std::sqrt(moment2_ / (n_ * (n_ - 1))) : QUIET_NAN;
}

// StdError() / Mean()
double OnlineStatistics::NormalizedError() const {
  std::lock_guard<std::mutex> guard(mutex_);
  return (n_ > 1) ? std::sqrt(moment2_ / (n_ * (n_ - 1))) / moment1_
                  : QUIET_NAN;
}
}  // namespace util
}  // namespace simulator
