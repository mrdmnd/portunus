#include <fstream>
#include <iostream>
#include <string>
#include <thread>

#include "proto/simulation.pb.h"
#include "simulate/engine/engine.h"

#include "absl/memory/memory.h"

#include "gflags/gflags.h"
#include "glog/logging.h"
#include "google/protobuf/text_format.h"

DEFINE_int32(threads, std::thread::hardware_concurrency(), "Num threads.");

using namespace simulate;

// Read the file at config_path, parse the contents into a SimulationConfig.
SimulationConfig ParseTextSimulationConfig(const std::string &config_path) {
  std::ifstream input(config_path, std::ios::in | std::ios::binary);
  CHECK(input) << "file at " << config_path << " could not be opened.";
  std::ostringstream contents;
  contents << input.rdbuf();
  input.close();

  SimulationConfig sc;
  google::protobuf::TextFormat::ParseFromString(contents.str(), &sc);
  return sc;
}

int main(int argc, char **argv) {
  gflags::ParseCommandLineFlags(&argc, &argv, true);
  CHECK_EQ(argc, 2) << "Required argument [config_path] missing.";
  const std::string config_path(argv[1]);
  const Engine e(FLAGS_threads);

  const SimulationConfig conf = ParseTextSimulationConfig(config_path);
  const SimulationResult result = e.Simulate(conf);

  std::string result_string;
  google::protobuf::TextFormat::PrintToString(result, &result_string);
  LOG(INFO) << "Simulation result:\n" << result_string;
  return 0;
}
