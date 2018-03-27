#pragma once

#include <string>
#include <vector>

#include "simulator/util/json.h"

namespace simulator {
namespace items {

class WowDB {
  static std::string BuildURL(int item_id, const std::vector<int> bonus_ids);
  static nlohmann::json GetJSON(int item_id, const std::vector<int> bonus_ids);
};
}  // namespace items
}  // namespace simulator
