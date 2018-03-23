#include <csignal>
#include <iostream>
#include <memory>
#include <string>
#include <thread>

#include "absl/strings/str_cat.h"
#include "gflags/gflags.h"
#include "glog/logging.h"
#include "google/protobuf/text_format.h"
#include "grpc++/grpc++.h"
#include "grpc/support/log.h"

#include "simulator/engine.h"

#include "proto/service.grpc.pb.h"

using grpc::Server;
using grpc::ServerAsyncResponseWriter;
using grpc::ServerBuilder;
using grpc::ServerCompletionQueue;
using grpc::ServerContext;
using grpc::Status;

using simulator::Engine;
using simulatorproto::SimulationRequest;
using simulatorproto::SimulationResponse;

DEFINE_string(host, "localhost", "");
DEFINE_string(port, "50051", "");
DEFINE_int32(threads, std::thread::hardware_concurrency(), "");

namespace {
class SimulationServiceImpl final
  : public simulatorproto::SimulationService::Service {
 public:
  explicit SimulationServiceImpl(const int num_threads) {
    engine_ = std::make_unique<Engine>(num_threads);
  }

  Status ConductSimulation(ServerContext* context,
                           const SimulationRequest* request,
                           SimulationResponse* response) override {
    LOG(INFO) << "Received simulation request.";
    std::string config;
    google::protobuf::TextFormat::PrintToString(request->config(), &config);
    LOG(INFO) << "Configuration: \n" << config;
    bool debug = false;
    response->mutable_result()->CopyFrom(
        engine_->Simulate(request->config(), debug));
    return Status::OK;
  }

 private:
  std::unique_ptr<Engine> engine_;
};
}  // namespace

void shutdown_handler(int signal) {
  LOG(INFO) << "Simulation service received CTRL-C, going down...";
  exit(signal);
}

int main(int argc, char** argv) {
  FLAGS_logtostderr = true;
  gflags::ParseCommandLineFlags(&argc, &argv, true);
  google::InitGoogleLogging(argv[0]);

  signal(SIGINT, shutdown_handler);
  signal(SIGTERM, shutdown_handler);

  SimulationServiceImpl service(FLAGS_threads);

  ServerBuilder builder;
  const std::string address = absl::StrCat(FLAGS_host, ":", FLAGS_port);
  builder.AddListeningPort(address, grpc::InsecureServerCredentials());
  builder.RegisterService(&service);

  std::unique_ptr<Server> server(builder.BuildAndStart());
  LOG(INFO) << "Simulation server listening at " << address;
  server->Wait();
  return 0;
}
