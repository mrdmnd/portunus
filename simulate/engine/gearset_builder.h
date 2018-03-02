#include "proto/player_config.pb.h"
#include "simulate/engine/player_combat_stats.h"

using namespace simulate;

// This class is responsible for taking a Gearset proto (list of WearableItems)
// and turning it into a collection of stats ratings, static effects, and on-use
// effects. We can source gear either through wowhead, or through the use of a
// local DBC file (NYI).
//
class GearsetBuilder {
 public:
  GearsetBuilder() = default;
  ~GearsetBuilder() = default;

  // Construct a
  GearsetBuilder(Gearset gearset_proto) : gearset_proto_(gearset_proto){};

  // Disallow {move,copy} {construction,assignment}.
  Gearset(const Gearset &other) = delete;
  Gearset(const Gearset &&other) = delete;
  Gearset &operator=(const Gearset &other) = delete;
  Gearset &operator=(const Gearset &&other) = delete;

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
