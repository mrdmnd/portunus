#include "proto/player_config.pb.h"

// These classes are responsible for taking protobuf descriptions of
// configuration and turning them into "parsed" forms.

namespace policygen {
namespace configparsers {

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
  explicit EquipmentSummary(simulatorproto::Gearset gearset_proto);

  // Disallow {move,copy} {construction,assignment}.
  EquipmentSummary(const EquipmentSummary& other) = delete;
  EquipmentSummary(const EquipmentSummary&& other) = delete;
  EquipmentSummary& operator=(const EquipmentSummary& other) = delete;
  EquipmentSummary& operator=(const EquipmentSummary&& other) = delete;

 private:
  const simulatorproto::Gearset gearset_proto_;
};

// This class contains the "parsed form" of the specific item.
// Data may come from local DB, blizzar API, wowhead, whatever.
class ParsedItem {
 public:
  ParsedItem(WearableItem item) : item_(item) {}

 private:
  const WearableItem item_;
};
}  // namespace configparsers
}  // namespace policygen
