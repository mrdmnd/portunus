// OnlineStatistics is a threadsafe aggregator of values that provides
// constant-time Mean() and Variance() computation at the cost of doing a bit
// more work during value aggregation. It also requires constant space.

#include <mutex>

class OnlineStatistics {
 public:
  OnlineStatistics() : n_(0.0), moment1_(0.0), moment2_(0.0){};
  ~OnlineStatistics() = default;

  // Explicitly disallow copy+move assign/construct.
  OnlineStatistics(const OnlineStatistics& other) = delete;
  OnlineStatistics(const OnlineStatistics&& other) = delete;
  OnlineStatistics& operator=(const OnlineStatistics& other) = delete;
  OnlineStatistics& operator=(const OnlineStatistics&& other) = delete;

  void AddValue(double value);
  double Mean() const;
  double Variance() const;

 private:
  mutable std::mutex mutex_;  // Marked mutable so Mean/Variance can be const.
  long n_;
  double moment1_;
  double moment2_;
};
