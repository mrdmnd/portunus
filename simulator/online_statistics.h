// OnlineStatistics is a threadsafe aggregator of values that provides
// constant-time Mean() and Variance() computation at the cost of doing a bit
// more work during value aggregation. It also requires constant space.

#include <mutex>

namespace policygen {
class OnlineStatistics {
 public:
  OnlineStatistics() : n_(0.0), moment1_(0.0), moment2_(0.0){};
  ~OnlineStatistics() = default;

  // Explicitly disallow copy+move assign/construct.
  OnlineStatistics(const OnlineStatistics& other) = delete;
  OnlineStatistics(const OnlineStatistics&& other) = delete;
  OnlineStatistics& operator=(const OnlineStatistics& other) = delete;
  OnlineStatistics& operator=(const OnlineStatistics&& other) = delete;

  // Add a value into this distribution.
  void AddValue(double value);

  // Returns the number of values that have been added.
  long Count() const;

  // Returns the sample mean from all values that have been added. If no values
  // have been added, returns 0.0
  double Mean() const;

  // Returns the sample variance from all values that have been added (this is
  // the "division by n-1" flavor). If fewer than two values have been added,
  // returns NaN.
  double Variance() const;

 private:
  mutable std::mutex mutex_;  // Marked mutable so Mean/Variance can be const.
  long n_;
  double moment1_;
  double moment2_;
};
}  // namespace policygen
