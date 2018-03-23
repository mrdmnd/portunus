#include <random>

#include "gtest/gtest.h"

#include "simulator/util/rng.h"

namespace simulator {
namespace util {
TEST(RNGTest, TestRoll) {
  RngEngine rng;
  bool or_results = false;
  // Do thirty rolls @ 30%, if any of them are true, we're gucci.
  // This is technically a flaky test, heh.
  for (int i = 0; i < 30; ++i) {
    or_results |= rng.Roll(0.30);
  }
  EXPECT_EQ(or_results, true);
}

TEST(RNGTest, TestUniformInt) {
  RngEngine rng;
  int result = rng.UniformInt(0, 10);
  EXPECT_GE(result, 0);
  EXPECT_LE(result, 10);
}

TEST(RNGTest, TestUniformDouble) {
  RngEngine rng;
  double result = rng.UniformDouble(0.0, 10.0);
  EXPECT_GE(result, 0.0);
  EXPECT_LE(result, 10.0);
}

TEST(RNGTest, TestNormal) {
  RngEngine rng;
  bool truncate_low = true;
  double result = rng.Normal(5.0, 2.0, truncate_low);
  EXPECT_GE(result, 0.0);
}
}  // namespace util
}  // namespace simulator
