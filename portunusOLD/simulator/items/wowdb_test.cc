#include <string>
#include <vector>

#include "glog/logging.h"
#include "gtest/gtest.h"
#include "rapidjson/document.h"
#include "rapidjson/stringbuffer.h"
#include "rapidjson/writer.h"

#include "simulator/items/wowdb.h"

namespace simulator {
namespace items {

TEST(WowDBTest, TestGetItemJSONNoBonus) {
  const int item_id = 128476;  // Fangs of the Devourer
  const std::vector<int> bonus_ids = {};
  const rapidjson::Document response = GetItemJSON(item_id, bonus_ids);
  rapidjson::StringBuffer buffer;
  rapidjson::Writer<rapidjson::StringBuffer> writer(buffer);
  response.Accept(writer);
  LOG(INFO) << "Test sees: ";
  LOG(INFO) << buffer.GetString();
}

}  // namespace items
}  // namespace simulator
