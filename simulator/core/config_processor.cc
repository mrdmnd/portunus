#include <functional>
#include <memory>

#include "glog/logging.h"

#include "simulator/core/config_processor.h"
#include "simulator/core/health_estimator.h"
#include "simulator/core/simulation_state.h"

#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"

using simulatorproto::EncounterConfig;
using simulatorproto::Gearset;
using simulatorproto::Policy;
using std::chrono::milliseconds;

namespace simulator {
namespace core {

EquipmentSummary::EquipmentSummary(const Gearset& gearset_proto) {
  LOG(INFO) << "Constructing equipment summary for gearset.";
}

void SpawnCallback(SimulationState* s) {
  const auto health_estimator = HealthEstimator::UniformHealthEstimator();
  const auto enemy = std::make_unique<Enemy>(health_estimator);
  s->enemies.push_back(std::move(enemy));
}

EncounterSummary::EncounterSummary(const EncounterConfig& encounter_proto) :
  time_target(encounter_proto.time_target()),
  time_variance(encounter_proto.time_variance()) {
  // Loop through proto events, set up appropriate RaidEvents with callbacks.
  for (int i = 0; i < encounter_proto.events_size(); ++i) {
    const auto& event = encounter_proto.events(i);
    switch (event.event_case()) {
      case simulatorproto::EncounterEvent::kSpawn: {
        const RaidEvent e(milliseconds(event.timestamp()), &SpawnCallback);
        raid_events.push_back(e);
        break;
      }
      case simulatorproto::EncounterEvent::kMovement: {
        break;
      }
      case simulatorproto::EncounterEvent::kLust: {
        break;
      }
      case simulatorproto::EncounterEvent::kStun: {
        break;
      }
      case simulatorproto::EncounterEvent::kDamage: {
        break;
      }
      case simulatorproto::EncounterEvent::kInvuln: {
        break;
      }
      default: { LOG(INFO) << "Attempting to add an unknown raid event."; }
    }
  }
}

PolicyFunctor::PolicyFunctor(const Policy& policy_proto) {
  LOG(INFO) << "Constructing policy functor.";
}

}  // namespace core
}  // namespace simulator
