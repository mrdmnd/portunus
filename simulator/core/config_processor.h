#pragma once

#include "simulator/core/raid_event.h"

#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"
#include "proto/simulation.pb.h"

using simulatorproto::EncounterConfig;
using simulatorproto::Gearset;
using simulatorproto::Policy;
using simulatorproto::WearableItem;

// These classes are responsible for taking protobuf descriptions of
// configuration and turning them into "parsed" forms.

namespace simulator {
namespace core {

// This class is responsible for taking a Gearset proto  and turning it into a
// collection of stats ratings, static effects, and on-use effects. This
// collection object is called a "EquipmentSummary" object. We can source gear
// either through wowhead, or through the use of a local DBC file (NYI).

class EquipmentSummary {
 public:
  //  Public members

  EquipmentSummary() = delete;

  // We can construct an equipment summary from a gearset protobuf.
  explicit EquipmentSummary(const Gearset& gearset_proto);

  // Default copy and move assignment and operator=
  EquipmentSummary(const EquipmentSummary& other) = default;
  EquipmentSummary(EquipmentSummary&& other) = default;
  EquipmentSummary& operator=(const EquipmentSummary& other) = default;
  EquipmentSummary& operator=(EquipmentSummary&& other) = default;
};

// This class is responsible for taking an EncounterConfig proto (list of raid
// events, etc) and turning it into an EncounterSummary, which is a sequence of
// raid events that correspond to the encounter.

class EncounterSummary {
 public:
  // Public members
  const int time_target;
  const double time_variance;
  std::vector<RaidEvent> raid_events;

  EncounterSummary() = delete;

  // We can construct an encounter summary from an EncounterConfig proto.
  explicit EncounterSummary(const EncounterConfig& encounter_proto);

  // Default copy and move assignment and operator=
  EncounterSummary(const EncounterSummary& other) = default;
  EncounterSummary(EncounterSummary&& other) = default;
  EncounterSummary& operator=(const EncounterSummary& other) = default;
  EncounterSummary& operator=(EncounterSummary&& other) = default;

 private:
};

// This class wraps a policy proto into something exposing the correct
// interface.
class PolicyFunctor {
 public:
  PolicyFunctor() = delete;
  explicit PolicyFunctor(const Policy& policy_proto);

  // Default copy and move assignment and operator=
  PolicyFunctor(const PolicyFunctor& other) = default;
  PolicyFunctor(PolicyFunctor&& other) = default;
  PolicyFunctor& operator=(const PolicyFunctor& other) = default;
  PolicyFunctor& operator=(PolicyFunctor&& other) = default;
};

}  // namespace core
}  // namespace simulator
