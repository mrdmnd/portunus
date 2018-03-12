
#include "gtest/gtest.h"

#include "simulator/core/health_estimator.h"

namespace simulator {
namespace core {

TEST(MapInterpolator, UniformInterpolation) {
  const std::map<double, double> control_points = {
      {0.0, 1.0}, {0.2, 0.8}, {0.4, 0.6}, {1.0, 0.0}};
  EXPECT_FLOAT_EQ(MapInterpolate(-0.1, control_points), 1.0);
  EXPECT_FLOAT_EQ(MapInterpolate(0.0, control_points), 1.0);
  EXPECT_FLOAT_EQ(MapInterpolate(0.1, control_points), 0.9);
  EXPECT_FLOAT_EQ(MapInterpolate(0.2, control_points), 0.8);
  EXPECT_FLOAT_EQ(MapInterpolate(0.4, control_points), 0.6);
  EXPECT_FLOAT_EQ(MapInterpolate(0.5, control_points), 0.5);
  EXPECT_FLOAT_EQ(MapInterpolate(0.9, control_points), 0.1);
  EXPECT_FLOAT_EQ(MapInterpolate(1.0, control_points), 0.0);
  EXPECT_FLOAT_EQ(MapInterpolate(1.1, control_points), 0.0);
}
}  // namespace core
}  // namespace simulator
