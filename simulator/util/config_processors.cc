#include "glog/logging.h"

#include "simulator/util/config_processors.h"

#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"

using simulatorproto::EncounterConfig;
using simulatorproto::Gearset;
using simulatorproto::Policy;

namespace simulator {
namespace util {

EquipmentSummary::EquipmentSummary(const Gearset& gearset_proto) :
  gearset_proto_(gearset_proto) {
  // LOG(INFO) << "Constructing equipment summary for gearset.";
}

EncounterSummary::EncounterSummary(const EncounterConfig& encounter_proto) :
  encounter_proto_(encounter_proto) {
  // LOG(INFO) << "Constructing encounter summary.";
}
PolicyFunctor::PolicyFunctor(const Policy& policy_proto) :
  policy_proto_(policy_proto) {
  // LOG(INFO) << "Constructing policy functor.";
}
}  // namespace util
}  // namespace simulator
