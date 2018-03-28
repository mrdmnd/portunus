#pragma once

#include <string>
#include <vector>

#include "simulator/util/json.h"

namespace simulator {
namespace items {
nlohmann::json GetItemJSON(int id, const std::vector<int> bonus_ids);
}  // namespace items
}  // namespace simulator
