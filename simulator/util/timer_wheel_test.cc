#include <functional>

#include "benchmark/benchmark.h"
#include "gtest/gtest.h"

#include "simulator/util/timer_wheel.h"

namespace simulator {
namespace util {
TEST(TimerWheelTest, Basic) {
  typedef std::function<void>() Callback;
  int count = 0;
  TimeEvent<Callback> timer([&count]() { ++count; });

  TimerWheel wheel;
  wheel.schedule(&timer, 5);
  wheel.advance(4);
  EXPECT_EQ(count, 0);
  wheel.advance(1);
  EXPECT_EQ(count, 1);

  // Test cancellation
  wheel.schedule(&timer, 5);
  timer.cancel();
  timers.advance(4);
  EXPECT_EQ(count, 1);
}
}  // namespace util
}  // namespace simulator
