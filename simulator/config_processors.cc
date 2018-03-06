#include "glog/logging.h"
#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"

#include "simulator/config_processors.h"

namespace policygen {
namespace configprocess {

EquipmentSummary::EquipmentSummary(
    const simulatorproto::Gearset& gearset_proto) :
  gearset_proto_(gearset_proto) {
  LOG(INFO) << "Constructing equipment summary for gearset.";
}

EncounterSummary::EncounterSummary(
    const simulatorproto::EncounterConfig& encounter_proto) :
  encounter_proto_(encounter_proto) {
  LOG(INFO) << "Constructing encounter summary.";
}
PolicyFunctor::PolicyFunctor(const simulatorproto::Policy& policy_proto) :
  policy_proto_(policy_proto) {
  LOG(INFO) << "Constructing policy functor.";
}
}  // namespace configprocess
}  // namespace policygen
