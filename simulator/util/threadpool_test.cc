#include <atomic>
#include <functional>

#include "gtest/gtest.h"

#include "simulator/util/threadpool.h"

namespace simulator {
namespace util {

void Increment(std::atomic_int& accumulator) { accumulator++; }

void TestKThreads(size_t kNumThreads) {
  ThreadPool pool(kNumThreads);
  EXPECT_EQ(pool.NumThreads(), kNumThreads);

  std::atomic_int accumulator(0);
  std::vector<std::future<void>> futures;
  for (size_t i = 0; i < kNumThreads; ++i) {
    futures.emplace_back(pool.Enqueue(Increment, std::ref(accumulator)));
  }
  for (auto& future : futures) {
    future.get();
  }
  EXPECT_EQ(accumulator.load(), kNumThreads);
}

TEST(ThreadpoolTest, SingleThread) { TestKThreads(1); }

TEST(ThreadpoolTest, ManyThreads) { TestKThreads(100); }

}  // namespace util
}  // namespace simulator
