#pragma once

#include "absl/strings/str_cat.h"
#include "cpr/cpr.h"
#include "glog/logging.h"

#include "simulator/util/json.h"

namespace items {

static constexpr std::string kWowDbEndpoint = "https://www.wowdb.com/api/item/";

class WowDB {
  static std::string BuildURL(int item_id, const std::vector<int> bonus_ids) {
    std::string base = absl::StrCat(kWowDbEndpoint, item_id);
    if (!bonus_ids.empty()) {
      StrAppend(&base, "?bonusIDs=");
      for (const auto bonus_id : bonus_ids) {
        StrAppend(&base, bonus_id, ",");
      }
    }
    return base;
  }

  static nlohmann::json GetItemJSON(int item_id,
                                    const std::vector<int> bonus_ids) {
    const auto url = BuildURL(item_id, modifiers);
    const auto response = cpr::Get(cpr::Url{url});

    if (!response.status_code == 200) {
      // ERROR!
      LOG(ERROR) << "Saw a non-200 response for url " << url;
      return nlohman::json;
    } else if (!response.header["content-type"] == "application/json") {
      LOG(ERROR) << "Saw a non-JSON response for url " << url;
      return nlohman::json;
    } else {
      LOG(INFO) << response.text;
      return nlohman::json::parse(response.text);
    }
  }
}
};  // namespace items
}  // namespace items
