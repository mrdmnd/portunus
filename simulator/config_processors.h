#pragma once

#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"
#include "proto/simulation.pb.h"

// These classes are responsible for taking protobuf descriptions of
// configuration and turning them into "parsed" forms.

namespace {
// This class contains the "parsed form" of the specific item.
// Data may come from local DB, blizzard API, wowhead, whatever.
class ParsedItem {
 public:
  explicit ParsedItem(simulatorproto::WearableItem item) : item_(item) {}

 private:
  const simulatorproto::WearableItem item_;
};
}  // namespace

namespace policygen {
namespace configprocess {

// This class is responsible for taking a Gearset proto (list of WearableItems)
// and turning it into a collection of stats ratings, static effects, and on-use
// effects. This collection object is called a "EquipmentSummary" object. We can
// source gear either through wowhead, or through the use of a local DBC file
// (NYI).
//
class EquipmentSummary {
 public:
  EquipmentSummary() = delete;

  // We can construct an equipment summary from a gearset protobuf.
  explicit EquipmentSummary(const simulatorproto::Gearset& gearset_proto);

  // Default copy and move assignment and operator=
  EquipmentSummary(const EquipmentSummary& other) = default;
  EquipmentSummary(EquipmentSummary&& other) = default;
  EquipmentSummary& operator=(const EquipmentSummary& other) = default;
  EquipmentSummary& operator=(EquipmentSummary&& other) = default;

 private:
  const simulatorproto::Gearset gearset_proto_;
};

// This class is responsible for taking an EncounterConfig proto (list of raid
// events, etc) and turning it into an EncounterSummary, which is a sequence of
// raid events that correspond to the encounter.

class EncounterSummary {
 public:
  EncounterSummary() = delete;

  // We can construct an encounter summary from an EncounterConfig proto.
  explicit EncounterSummary(
      const simulatorproto::EncounterConfig& encounter_proto);

  // Default copy and move assignment and operator=
  EncounterSummary(const EncounterSummary& other) = default;
  EncounterSummary(EncounterSummary&& other) = default;
  EncounterSummary& operator=(const EncounterSummary& other) = default;
  EncounterSummary& operator=(EncounterSummary&& other) = default;

 private:
  const simulatorproto::EncounterConfig encounter_proto_;
};

// This class wraps a policy proto into something with a Choose() method.
class PolicyFunctor {
 public:
  PolicyFunctor() = delete;
  explicit PolicyFunctor(const simulatorproto::Policy& policy_proto);

  // Default copy and move assignment and operator=
  PolicyFunctor(const PolicyFunctor& other) = default;
  PolicyFunctor(PolicyFunctor&& other) = default;
  PolicyFunctor& operator=(const PolicyFunctor& other) = default;
  PolicyFunctor& operator=(PolicyFunctor&& other) = default;

 private:
  const simulatorproto::Policy policy_proto_;
};

}  // namespace configprocess
}  // namespace policygen
