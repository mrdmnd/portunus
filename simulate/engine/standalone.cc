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

using namespace simulate;

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

int main(int argc, char** argv) {
  gflags::ParseCommandLineFlags(&argc, &argv, true);
  CHECK_EQ(argc, 2) << "Required argument [config_path] missing.";
  const Engine e();
  const SimulationConfig conf = ParseConfig<SimulationConfig>(argv[1]);
  const SimulationResult result = e.Simulate(conf);

  std::string result_string;
  google::protobuf::TextFormat::PrintToString(result, &result_string);
  LOG(INFO) << "Simulation result:\n" << result_string;
  return 0;
}
