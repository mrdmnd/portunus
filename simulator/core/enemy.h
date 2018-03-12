#pragma once

#include "simulator/core/actor.h"
#include "simulator/core/constants.h"
#include "simulator/core/health_estimator.h"

using namespace simulator::core::constants;
using namespace simulator::core::enums;

namespace simulator {
namespace core {
class Enemy : Actor {
 public:
  Enemy(const HealthEstimator& health_estimator) :
    health_estimator_(health_estimator) {
    LOG(INFO) << "Spawning enemy.";
  }
  ~Enemy() { LOG(INFO) << "Destroying enemy."; };

 private:
  const HealthEstimator health_estimator_;
};
}  // namespace core
}  // namespace simulator
