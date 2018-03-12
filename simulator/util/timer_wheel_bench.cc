#include <functional>
#include <random>

#include "benchmark/benchmark.h"

#include "simulator/util/timer_wheel.h"
namespace simulator {
namespace util {

using Callback = std::function<void(int*)>;

static void BM_InsertTimers(benchmark::State& state) {
  int count = 0;
  constexpr int kMaxSchedulingOffset = 120000;  // Two minutes, ish?
  TimerWheel wheel;
  std::default_random_engine gen;
  std::uniform_int_distribution<Tick> distribution(1, kMaxSchedulingOffset);

  int increment = 1;
  for (auto _ : state) {
    for (int i = 0; i < state.range(0); ++i) {
      TimerEvent<Callback, int*> timer(
          {[&count](int* inc) { count += *inc; }, &increment});
      wheel.Schedule(&timer, distribution(gen));
    }
  }
}
BENCHMARK(BM_InsertTimers)->RangeMultiplier(2)->Range(2, 1 << 15);

static void BM_InsertTimersAndAdvance(benchmark::State& state) {
  int count = 0;
  constexpr int kMaxSchedulingOffset = 2000;  // Two seconds max
  TimerWheel wheel;
  std::default_random_engine gen;
  std::uniform_int_distribution<Tick> distribution(1, kMaxSchedulingOffset);

  int increment = 1;
  for (auto _ : state) {
    for (int i = 0; i < state.range(0); ++i) {
      TimerEvent<Callback, int*> timer(
          {[&count](int* inc) { count += *inc; }, &increment});
      wheel.Schedule(&timer, distribution(gen));
      wheel.Advance(wheel.TicksUntilNextEvent());
    }
  }
}
BENCHMARK(BM_InsertTimersAndAdvance)->RangeMultiplier(2)->Range(2, 1 << 11);

}  // namespace util
}  // namespace simulator

BENCHMARK_MAIN();

/* To run, invoke

sudo cpupower frequency-set --governor performance                          && \
bazel build --config=optg simulator:timer_wheel_bench                       && \
perf record ./bazel-bin/simulator/timer_wheel_bench                         && \
sudo cpupower frequency-set --governor powersave                            && \
perf report && rm perf.data

or

sudo cpupower frequency-set --governor performance                          && \
bazel build --config=optg simulator:timer_wheel_bench                       && \
valgrind --tool=callgrind --dump-instr=yes                                     \
./bazel-bin/simulator/timer_wheel_bench                                     && \
sudo cpupower frequency-set --governor powersave                            && \
kcachegrind callgrind.out.* && rm callgrind.out.*

*/
