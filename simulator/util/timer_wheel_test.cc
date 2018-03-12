#include <functional>

#include "gtest/gtest.h"

#include "simulator/util/timer_wheel.h"

namespace simulator {
namespace util {

using Callback = std::function<void(int*)>;

// Test that we can insert a timer into our wheel, advance the wheel, and see
// that the timer has Execute()'d.
TEST(TimerWheelTest, Timer_BasicFunctionality) {
  int count = 0;
  int increment = 1;
  TimerEvent<Callback, int*> timer(
      {[&count](int* inc) { count += *inc; }, &increment});
  TimerWheel wheel;

  wheel.Schedule(&timer, 5);
  wheel.Advance(4);
  EXPECT_EQ(count, 0);
  wheel.Advance(1);
  EXPECT_EQ(count, 1);
}

// Test that we can insert a timer into our wheel, cancel the timer, advance the
// wheel, and then see that the timer has not Execute()'d.
TEST(TimerWheelTest, Timer_Cancel) {
  int count = 0;
  int increment = 1;
  TimerEvent<Callback, int*> timer(
      {[&count](int* inc) { count += *inc; }, &increment});
  TimerWheel wheel;

  wheel.Schedule(&timer, 5);
  timer.Cancel();
  wheel.Advance(4);
  EXPECT_EQ(count, 0);
  wheel.Advance(1);
  EXPECT_EQ(count, 0);
}

// Test that we can insert a timer into our wheel, and verify that it found a
// non-null slot to live in.
TEST(TimerWheelTest, Timer_Active) {
  int count = 0;
  int increment = 1;
  TimerEvent<Callback, int*> timer(
      {[&count](int* inc) { count += *inc; }, &increment});
  TimerWheel wheel;

  wheel.Schedule(&timer, 5);
  EXPECT_TRUE(timer.Active());
}

// Test that we can insert a timer into our wheel, and we can get the exact tick
// that the event is scheduled to execute on.
TEST(TimerWheelTest, Timer_ScheduledAt) {
  int count = 0;
  int increment = 1;
  TimerEvent<Callback, int*> timer(
      {[&count](int* inc) { count += *inc; }, &increment});
  TimerWheel wheel;

  wheel.Schedule(&timer, 5);
  EXPECT_EQ(timer.ScheduledAt(), 5);
}

// Test that we can appropriate reschedule an event.
TEST(TimerWheelTest, Timer_Reschedule) {
  int count = 0;
  int increment = 1;
  TimerEvent<Callback, int*> timer(
      {[&count](int* inc) { count += *inc; }, &increment});
  TimerWheel wheel;

  wheel.Schedule(&timer, 5);
  timer.Reschedule(10);
  EXPECT_EQ(timer.ScheduledAt(), 10);
}

// Test that we can move time forwards, with no events queued.
TEST(TimerWheelTest, TimerWheel_AdvanceBasic) {
  TimerWheel wheel;
  wheel.Advance(5);
  EXPECT_EQ(wheel.Now(), 5);
}

// Our wheel provides an interface that lets us cap the maximum amount of work
// done in a single Advance() call. This might be useful? Test it.
TEST(TimerWheelTest, TimerWheel_AdvanceBounded) {
  constexpr int kNumTimers = 1000;
  constexpr int kMaxExecute = 10;
  int count = 0;
  int increment = 1;
  TimerWheel wheel;

  std::vector<std::unique_ptr<TimerEvent<Callback, int*>>> timers;
  Callback c([&count](int* inc) { count += *inc; });

  for (int i = 0; i < kNumTimers; ++i) {
    timers.emplace_back(
        std::make_unique<TimerEvent<Callback, int*>>(TimerTask{c, &increment}));
  }
  for (int i = 0; i < kNumTimers; ++i) {
    wheel.Schedule(timers[i].get(), 5);
  }

  wheel.Advance(6, kMaxExecute);
  EXPECT_EQ(count, kMaxExecute);
}

// Our wheel provides a method that lets us figure out how long until the next
// scheduled event occurs.
TEST(TimerWheelTest, TimerWheel_TicksUntilNextEventBasic) {
  int count = 0;
  int increment = 1;
  TimerEvent<Callback, int*> timer(
      {[&count](int* inc) { count += *inc; }, &increment});
  TimerWheel wheel;

  wheel.Schedule(&timer, 5);
  EXPECT_EQ(wheel.TicksUntilNextEvent(), 5);
  wheel.Advance(1);
  EXPECT_EQ(wheel.Now(), 1);
  EXPECT_EQ(wheel.TicksUntilNextEvent(), 4);
  // Advance to timestep 5, which fires the event and clears our wheel.
  wheel.Advance(4);
  EXPECT_EQ(wheel.Now(), 5);
  // No events left, time is infinity.
  EXPECT_EQ(wheel.TicksUntilNextEvent(), std::numeric_limits<Tick>::max());
}

// Run the tests from the original github page.
TEST(TimerWheelTest, TestNonHierarchicalFromOriginalGithub) {
  int count = 0;
  int increment = 1;
  TimerEvent<Callback, int*> timer(
      {[&count](int* inc) { count += *inc; }, &increment});
  TimerWheel wheel;

  // Unscheduled timer does nothing.
  wheel.Advance(10);
  EXPECT_EQ(count, 0);
  EXPECT_TRUE(!timer.Active());

  // Schedule timer, should trigger at right time.
  wheel.Schedule(&timer, 5);
  EXPECT_TRUE(timer.Active());
  wheel.Advance(5);
  EXPECT_EQ(count, 1);

  // Only trigger once, not repeatedly (even if wheel wraps
  // around).
  wheel.Advance(256);
  EXPECT_EQ(count, 1);

  // ... unless, of course, the timer gets scheduled again.
  wheel.Schedule(&timer, 5);
  wheel.Advance(5);
  EXPECT_EQ(count, 2);

  // Canceled timers don't run.
  wheel.Schedule(&timer, 5);
  timer.Cancel();
  EXPECT_TRUE(!timer.Active());
  wheel.Advance(10);
  EXPECT_EQ(count, 2);

  // Test wraparound
  wheel.Advance(250);
  wheel.Schedule(&timer, 5);
  wheel.Advance(10);
  EXPECT_EQ(count, 3);

  // Timers that are scheduled multiple times only run at the last
  // scheduled tick.
  wheel.Schedule(&timer, 5);
  wheel.Schedule(&timer, 10);
  wheel.Advance(5);
  EXPECT_EQ(count, 3);
  wheel.Advance(5);
  EXPECT_EQ(count, 4);

  // Timer can safely be canceled multiple times.
  wheel.Schedule(&timer, 5);
  timer.Cancel();
  timer.Cancel();
  EXPECT_TRUE(!timer.Active());
  wheel.Advance(10);
  EXPECT_EQ(count, 4);

  {
    TimerEvent<Callback, int*> timer2(
        {[&count](int* inc) { count += *inc; }, &increment});
    wheel.Schedule(&timer2, 5);
  }

  wheel.Advance(10);
  EXPECT_EQ(count, 4);
}
}  // namespace util
}  // namespace simulator
