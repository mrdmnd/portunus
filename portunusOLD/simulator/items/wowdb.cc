#include <curl/curl.h>

#include "absl/strings/str_cat.h"
#include "glog/logging.h"
#include "rapidjson/document.h"

#include "simulator/items/wowdb.h"
namespace simulator {
namespace items {

static constexpr const char* kWowDbEndpoint = "https://www.wowdb.com/api/item/";

size_t WriteFunction(void* ptr, size_t size, size_t nmemb, std::string* data) {
  data->append((char*) ptr, size * nmemb);
  return size * nmemb;
}

std::string FetchResponse(const char* url) {
  // Uses CURL to do a super primitive GET request.
  auto curl = curl_easy_init();
  curl_easy_setopt(curl, CURLOPT_URL, url);

  std::string response_string;
  std::string header_string;
  curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, WriteFunction);
  curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response_string);
  curl_easy_setopt(curl, CURLOPT_HEADERDATA, &header_string);

  long response_code;
  CURLcode res = curl_easy_perform(curl);
  curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &response_code);
  if (res != CURLE_OK || response_code != 200) {
    LOG(ERROR) << "curl request failed: " << curl_easy_strerror(res);
    return "";
  }
  curl_easy_cleanup(curl);
  curl = NULL;

  return response_string;
}

rapidjson::Document GetItemJSON(const int id,
                                const std::vector<int> bonus_ids) {
  std::string url = absl::StrCat(kWowDbEndpoint, id);
  if (!bonus_ids.empty()) {
    absl::StrAppend(&url, "?bonusIDs=");
    for (const auto bonus_id : bonus_ids) {
      absl::StrAppend(&url, bonus_id, ",");
    }
  }

  // WowDB returns a JSON with leading and trailing () parentheses, so we trim.
  const std::string response = FetchResponse(url.c_str());
  const auto& trimmed = response.substr(1, response.size() - 2);
  rapidjson::Document d;
  d.Parse(trimmed.c_str());
  return d;
}
}  // namespace items
}  // namespace simulator
