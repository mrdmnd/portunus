#pragma once

#include "simulator/core/actor.h"
#include "simulator/core/constants.h"
#include "simulator/core/health_estimator.h"

namespace simulator {
namespace core {
class Enemy : Actor {
 public:
  Enemy(const std::string& name, const HealthEstimator& health_estimator) :
    name_(name),
    health_estimator_(health_estimator) {
    LOG(INFO) << "Spawning enemy named " << name;
  }
  ~Enemy() { LOG(INFO) << "Destroying enemy."; };

 private:
  const std::string name_;
  const HealthEstimator health_estimator_;
};
}  // namespace core
}  // namespace simulator
