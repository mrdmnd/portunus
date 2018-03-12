#pragma once

#include <chrono>

namespace simulator {
namespace core {

using std::chrono::milliseconds;

// A Cooldown is a timer preventing players from using abilities too frequently.
class Cooldown {
 public:
  explicit Cooldown(const milliseconds max_duration) :
    max_duration_(max_duration){};

  inline bool IsReady() const { return cur_duration_ == milliseconds::zero(); }

  inline milliseconds Remaining() const { return cur_duration_; }

  inline void ResetToMax() { cur_duration_ = max_duration_; }

  inline void RemoveTime(const milliseconds time) { cur_duration_ -= time; }

 private:
  const std::chrono::milliseconds max_duration_;
  std::chrono::milliseconds cur_duration_;
};
}  // namespace core
}  // namespace simulator
