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

EncounterSummary::EncounterSummary(const EncounterConfig& encounter_proto) :
  time_target(encounter_proto.time_target()),
  time_variance(encounter_proto.time_variance()) {
  // Loop through proto events, set up appropriate RaidEvents with callbacks.
  for (int i = 0; i < encounter_proto.events_size(); ++i) {
    const auto& event_proto = encounter_proto.events(i);
    switch (event_proto.event_case()) {
      case simulatorproto::EncounterEvent::kSpawn: {
        std::function<void(SimulationState*)> cb =
            [&event_proto](SimulationState* s) {
              const auto enemy_proto = event_proto.spawn().enemy();
              const auto enemy_name = enemy_proto.name();
              const auto estimator_proto = enemy_proto.health_estimator();
              const auto estimator = HealthEstimator::UniformHealthEstimator();
              s->enemies.push_back(std::make_unique<Enemy>(estimator));
            };
        Event e(milliseconds(event_proto.timestamp()), cb);
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
