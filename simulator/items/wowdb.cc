#include <curl.curl.h>

#include "absl/strings/str_cat.h"
#include "glog/logging.h"

#include "simulator/util/json.h"

#include "simulator/items/wowdb.h"
namespace simulator {
namespace items {

static constexpr const char* kWowDbEndpoint = "https://www.wowdb.com/api/item/";

size_t WriteFunction(void* ptr, size_t size, size_t nmemb, std::string* data) {
  data->append((char*) ptr, size * nmemb);
  return size * nmemb;
}

nlohmann::json GetItemJSON(const int id, const std::vector<int> bonus_ids) {
  std::string url = absl::StrCat(kWowDbEndpoint, id);
  if (!bonus_ids.empty()) {
    absl::StrAppend(&url, "?bonusIDs=");
    for (const auto bonus_id : bonus_ids) {
      absl::StrAppend(&url, bonus_id, ",");
    }
  }

  auto curl = curl_easy_init();
  curl_easy_setopt(curl, CURLOPT_URL, url.c_str());

  std::string response_string;
  std::string header_string;
  curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, WriteFunction);
  curl_easy_setopt(curl, CURLOPT_WRITEDATA, &response_string);
  curl_easy_setopt(curl, CURLOPT_HEADERDATA, &header_string);
  curl_easy_setopt(curl, CURLOPT_USE_SSL, CURLUSESSL_NONE);  // workaround!
  curl_easy_setopt(curl, CURLOPT_SSL_VERIFYPEER, false);
  curl_easy_setopt(curl, CURLOPT_SSL_VERIFYHOST, 0L);
  // curl_easy_setopt(curl, CURLOPT_SSLVERSION, CURL_SSLVERSION_DEFAULT);

  long response_code;
  double elapsed;
  curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &response_code);
  curl_easy_getinfo(curl, CURLINFO_TOTAL_TIME, &elapsed);
  CURLcode res = curl_easy_perform(curl);
  if (res != CURLE_OK) {
    LOG(ERROR) << "curl_easy_perform() failed: " << curl_easy_strerror(res);
    nlohmann::json empty;
    return empty;
  }
  curl_easy_cleanup(curl);
  curl = NULL;

  LOG(INFO) << "Response code: " << response_code;
  LOG(INFO) << "Response: " << response_string;

  if (response_code != 200) {
    LOG(ERROR) << "Saw a non-200 response for url " << url;
    nlohmann::json empty;
    return empty;
  }

  // WowDB returns a JSON with leading and trailing () parentheses, so we trim.
  const auto& trimmed = response_string.substr(1, response_string.size() - 2);
  LOG(INFO) << trimmed;
  return nlohmann::json::parse(trimmed);
}
}  // namespace items
}  // namespace simulator
