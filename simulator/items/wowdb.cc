#include "absl/strings/str_cat.h"
#include "cpr/cpr.h"
#include "glog/logging.h"

#include "simulator/util/json.h"

#include "simulator/items/wowdb.h"
namespace simulator {
namespace items {

static constexpr const char* kWowDbEndpoint = "https://www.wowdb.com/api/item/";

std::string WowDB::BuildURL(const int item_id,
                            const std::vector<int> bonus_ids) {
  std::string base = absl::StrCat(kWowDbEndpoint, item_id);
  if (!bonus_ids.empty()) {
    absl::StrAppend(&base, "?bonusIDs=");
    for (const auto bonus_id : bonus_ids) {
      absl::StrAppend(&base, bonus_id, ",");
    }
  }
  return base;
}

nlohmann::json GetJSON(const int item_id, const std::vector<int> bonus_ids) {
  const auto url = WowDB::BuildURL(item_id, bonus_ids);
  const auto response = cpr::Get(cpr::Url{url});

  if (response.status_code != 200) {
    LOG(ERROR) << "Saw a non-200 response for url " << url;
    nlohmann::json empty;
    return empty;
  } else if (response.header.at("content-type") != "application/json") {
    LOG(ERROR) << "Saw a non-JSON response for url " << url;
    nlohmann::json empty;
    return empty;
  }

  LOG(INFO) << response.text;
  return nlohmann::json::parse(response.text);
}
}  // namespace items
}  // namespace simulator
