#include <fstream>
#include <iostream>
#include <string>
#include <thread>

#include "absl/memory/memory.h"
#include "gflags/gflags.h"
#include "glog/logging.h"
#include "google/protobuf/text_format.h"

#include "simulator/engine.h"

#include "proto/simulation.pb.h"

using simulator::Engine;
using simulatorproto::SimulationConfig;
using simulatorproto::SimulationResult;

DEFINE_int32(threads, std::thread::hardware_concurrency(), "");
DEFINE_bool(debug, false, "If true, runs a single thread for one iteration.");

namespace {
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
  const Engine e(FLAGS_threads);
  const SimulationConfig conf = ParseConfig<SimulationConfig>(argv[1]);
  LOG(INFO) << "Simulating in standalone program.";
  const SimulationResult result = e.Simulate(conf, FLAGS_debug);

  std::string result_string;
  google::protobuf::TextFormat::PrintToString(result, &result_string);
  LOG(INFO) << "Simulation result:\n" << result_string;
  return 0;
}
