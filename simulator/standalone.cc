#include <fstream>
#include <iostream>
#include <string>
#include <thread>

#include "proto/simulation.pb.h"
#include "simulator/engine.h"

#include "absl/memory/memory.h"

#include "gflags/gflags.h"
#include "glog/logging.h"
#include "google/protobuf/text_format.h"

DEFINE_int32(threads, std::thread::hardware_concurrency(), "");

namespace {
// Read the file at config_path, parse the contents into a SimulationConfig.
template <class T>
T ParseConfig(const std::string& config_path) {
  std::ifstream input(config_path, std::ios::in | std::ios::binary);
  CHECK(input) << "file at " << config_path << " could not be opened.";
  std::ostringstream contents;
  contents << input.rdbuf();
  input.close();

  T conf;
  google::protobuf::TextFormat::ParseFromString(contents.str(), &conf);
  return conf;
}
}  // namespace

int main(int argc, char** argv) {
  FLAGS_logtostderr = true;
  gflags::ParseCommandLineFlags(&argc, &argv, true);
  google::InitGoogleLogging(argv[0]);

  CHECK_EQ(argc, 2) << "Required argument [config_path] missing.";
  const policygen::Engine e(FLAGS_threads);
  const simulatorproto::SimulationConfig conf =
      ParseConfig<simulatorproto::SimulationConfig>(argv[1]);
  const simulatorproto::SimulationResult result = e.Simulate(conf);

  std::string result_string;
  google::protobuf::TextFormat::PrintToString(result, &result_string);
  LOG(INFO) << "Simulation result:\n" << result_string;
  return 0;
}
