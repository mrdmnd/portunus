#pragma once
#include <string>
#include <vector>

#include "rapidjson/document.h"

namespace simulator {
namespace items {
rapidjson::Document GetItemJSON(int id, const std::vector<int> bonus_ids);
}  // namespace items
}  // namespace simulator
