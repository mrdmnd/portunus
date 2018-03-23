#pragma once

#include <chrono>

namespace simulator {
namespace core {

// A Cooldown is a timer preventing players from using abilities too frequently.
class Cooldown {
 public:
  explicit Cooldown(const std::chrono::milliseconds max_duration) :
    max_duration_(max_duration),
    cur_duration_(std::chrono::milliseconds::zero()){};

  inline bool IsReady() const {
    return cur_duration_ == std::chrono::milliseconds::zero();
  }

  inline std::chrono::milliseconds Remaining() const { return cur_duration_; }

  inline void PutOnCooldown() { cur_duration_ = max_duration_; }

  inline void ModifyTime(const std::chrono::milliseconds time_delta) {
    cur_duration_ += time_delta;
  }

 private:
  const std::chrono::milliseconds max_duration_;
  std::chrono::milliseconds cur_duration_;
};
}  // namespace core
}  // namespace simulator
