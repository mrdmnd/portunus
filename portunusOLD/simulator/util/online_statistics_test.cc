#include <cmath>
#include <thread>

#include "gtest/gtest.h"

#include "simulator/util/online_statistics.h"

namespace simulator {
namespace util {
TEST(OnlineStatisticsTest, SingleThread_Empty) {
  OnlineStatistics os;
  EXPECT_EQ(os.Count(), 0);
  EXPECT_EQ(os.Mean(), 0);
  EXPECT_TRUE(std::isnan(os.Variance()));
  EXPECT_TRUE(std::isnan(os.StdError()));
  EXPECT_TRUE(std::isnan(os.NormalizedError()));
}

TEST(OnlineStatisticsTest, SingleThread_AddOnce) {
  OnlineStatistics os;
  os.AddValue(1.0);
  EXPECT_EQ(os.Count(), 1);
  EXPECT_FLOAT_EQ(os.Mean(), 1.0);
  EXPECT_TRUE(std::isnan(os.Variance()));
  EXPECT_TRUE(std::isnan(os.StdError()));
  EXPECT_TRUE(std::isnan(os.NormalizedError()));
}

TEST(OnlineStatisticsTest, SingleThread_AddTwice) {
  OnlineStatistics os;
  os.AddValue(1.0);
  os.AddValue(2.0);
  EXPECT_EQ(os.Count(), 2);
  EXPECT_FLOAT_EQ(os.Mean(), 1.5);
  EXPECT_FLOAT_EQ(os.Variance(), 0.5);
  EXPECT_FLOAT_EQ(os.StdError(), 0.5);
  EXPECT_FLOAT_EQ(os.NormalizedError(), 1.0 / 3.0);
}

TEST(OnlineStatisticsTest, SingleThread_AddThrice) {
  OnlineStatistics os;
  os.AddValue(1.0);
  os.AddValue(2.0);
  os.AddValue(3.0);
  EXPECT_EQ(os.Count(), 3);
  EXPECT_FLOAT_EQ(os.Mean(), 2.0);
  EXPECT_FLOAT_EQ(os.Variance(), 1.0);
  EXPECT_FLOAT_EQ(os.StdError(), std::sqrt(1.0 / 3.0));
  EXPECT_FLOAT_EQ(os.NormalizedError(), std::sqrt(1.0 / 3.0) / 2.0);
}

TEST(OnlineStatisticsTest, ManyThreads_AddOnce) {
  constexpr int kNumThreads = 100;
  constexpr int kNumTrials = 50;

  // Try to force a race condition by hammering for kNumTrials.
  // You can prove to yourself that this works by removing the lock_guards on
  // the OnlineStatistics impl and see that this test usually fails.
  for (int t = 0; t < kNumTrials; ++t) {
    OnlineStatistics os;
    std::vector<std::thread> threads;
    threads.reserve(kNumThreads);
    for (int i = 1; i <= kNumThreads; ++i) {
      threads.emplace_back([i, &os]() { os.AddValue(i); });
    }
    for (auto& thread : threads) {
      thread.join();
    }
    EXPECT_EQ(os.Count(), kNumThreads);

    // Mean of integers 1-kNumThreads is (kNumThreads+1) / 2
    EXPECT_FLOAT_EQ(os.Mean(), (kNumThreads + 1) / 2.0);

    // Explicit formula for sample variance of the range 1-kNumThreads:
    // Sample variance is (A - B/K) / (K-1), with
    // A = Sum of (first K squares) is K*(K+1)*(2K+1)/6
    // = K^3 / 3 + K^2 / 2 + K / 6
    // B = Square of (sum of first K integers) is K*K*(K+1)*(K+1)/4
    // = K^4 / 4 + K^3 / 2 + K^2 / 4
    // Grind through the algebra, it ends up K*(K+1)/12
    EXPECT_FLOAT_EQ(os.Variance(), kNumThreads * (kNumThreads + 1) / 12.0);

    // Standard error is sqrt(variance / n), in this case, that's
    // sqrt((kNumThreads+1)/12)
    EXPECT_FLOAT_EQ(os.StdError(), std::sqrt((kNumThreads + 1) / 12.0));

    // Normalized error is StdError / Mean, in this case, that's
    // 2 * std::sqrt((kNumThreads + 1) / 12.0) / (kNumThreads + 1)
    EXPECT_FLOAT_EQ(
        os.NormalizedError(),
        2 * std::sqrt((kNumThreads + 1) / 12.0) / (kNumThreads + 1));
  }
}
}  // namespace util
}  // namespace simulator
