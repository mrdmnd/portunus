#include "proto/player_config.pb.h"

namespace policygen {
namespace configparsers {
class EquipmentSummary {
 public:
  EquipmentSummary() = delete;
  explicit EquipmentSummary(simulatorproto::Gearset gearset_proto)
      : gearset_proto_(gearset_proto){};

  // Disallow {move,copy} {construction,assignment}.
  ParseEquipmentBuilder(const ParsedEquipmentBuilder& other) = delete;
  ParsedEquipmentBuilder(const ParsedEquipmentBuilder&& other) = delete;
  ParsedEquipmentBuilder& operator=(const ParsedEquipmentBuilder& other) =
      delete;
  ParsedEquipmentBuilder& operator=(const ParsedEquipmentBuilder&& other) =
      delete;

 private:
  const Gearset gearset_proto_;
};

// This class contains the "parsed form" of the specific item.
// Data may come from local DB, blizzar API, wowhead, whatever.
class ParsedItem {
 public:
  ParsedItem(WearableItem item) : item_(item) {}

 private:
  const WearableItem item_;
}
}  // namespace configparsers
}  // namespace policygen
