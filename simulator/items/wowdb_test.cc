#include <string>
#include <vector>

#include "glog/logging.h"
#include "gtest/gtest.h"

#include "simulator/util/json.h"

#include "simulator/items/wowdb.h"

namespace simulator {
namespace items {
TEST(WowDBTest, TestBuildURLNoBonus) {
  const int item_id = 10;
  const std::vector<int> bonus_ids;
  const std::string built_url = WowDB::BuildURL(item_id, bonus_ids);
  EXPECT_EQ(built_url, "https://www.wowdb.com/api/item/10");
}
TEST(WowDBTest, TestBuildURLSingleBonus) {
  const int item_id = 10;
  const std::vector<int> bonus_ids = {1};
  const std::string built_url = WowDB::BuildURL(item_id, bonus_ids);
  EXPECT_EQ(built_url, "https://www.wowdb.com/api/item/10?bonusIDs=1,");
}
TEST(WowDBTest, TestBuildURLMultiBonus) {
  const int item_id = 10;
  const std::vector<int> bonus_ids = {1, 2};
  const std::string built_url = WowDB::BuildURL(item_id, bonus_ids);
  EXPECT_EQ(built_url, "https://www.wowdb.com/api/item/10?bonusIDs=1,2,");
}

TEST(WowDBTest, TestGetJSONNoBonus) {
  const int item_id = 128476;  // Fangs of the Devourer
  const std::vector<int> bonus_ids;
  const nlohmann::json response = WowDB::GetJSON(item_id, bonus_ids);
  LOG(INFO) << response.dump();
}

}  // namespace items
}  // namespace simulator
