#include <math.h>  // cmath

#include "gtest/gtest.h"

#include "simulate/engine/online_statistics.h"

TEST(OnlineStatisticsTest, SingleThread_Empty) {
  OnlineStatistics os;
  EXPECT_EQ(os.Mean(), 0);
  EXPECT_TRUE(isnan(os.Variance()));
}

TEST(OnlineStatisticsTest, SingleThread_AddOnce) {
  OnlineStatistics os;
  os.AddValue(1.0);
  EXPECT_EQ(os.Mean(), 1.0);
  EXPECT_TRUE(isnan(os.Variance()));
}

TEST(OnlineStatisticsTest, SingleThread_AddTwice) {
  OnlineStatistics os;
  os.AddValue(1.0);
  os.AddValue(2.0);
  EXPECT_EQ(os.Mean(), 1.5);
  EXPECT_EQ(os.Variance(), 0.5);
}
