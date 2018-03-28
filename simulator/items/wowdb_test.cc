#include <string>
#include <vector>

#include "glog/logging.h"
#include "gtest/gtest.h"

#include "simulator/util/json.h"

#include "simulator/items/wowdb.h"

namespace simulator {
namespace items {

TEST(WowDBTest, TestGetItemJSONNoBonus) {
  const int item_id = 128476;  // Fangs of the Devourer
  const std::vector<int> bonus_ids = {};
  const nlohmann::json response = GetItemJSON(item_id, bonus_ids);
  LOG(INFO) << response.dump();
}

}  // namespace items
}  // namespace simulator
