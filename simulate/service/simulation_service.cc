#include <csignal>
#include <iostream>
#include <memory>
#include <string>
#include <thread>

#include "proto/service.grpc.pb.h"
#include "simulate/engine/engine.h"

#include "absl/memory/memory.h"
#include "absl/strings/str_cat.h"

#include "gflags/gflags.h"
#include "glog/logging.h"
#include "grpc++/grpc++.h"
#include "grpc/support/log.h"

DEFINE_string(host, "localhost", "Which address to serve from.");
DEFINE_string(port, "50051", "Which port to serve requests from.");
DEFINE_int32(threads, std::thread::hardware_concurrency(), "Num threads.");

using grpc::Server;
using grpc::ServerAsyncResponseWriter;
using grpc::ServerBuilder;
using grpc::ServerCompletionQueue;
using grpc::ServerContext;
using grpc::Status;

using namespace simulate;

enum class CallStatus { CREATE, PROCESS, FINISH };

class SimulationServiceImpl final : public SimulationService::Service {
public:
  explicit SimulationServiceImpl(const int num_threads) {
    engine_ = absl::make_unique<Engine>(num_threads);
  }

  Status ConductSimulation(ServerContext *context,
                           const SimulationRequest *request,
                           SimulationResponse *response) override {
    response->mutable_result()->CopyFrom(engine_->Simulate(request->config()));
    return Status::OK;
  }

private:
  std::unique_ptr<Engine> engine_;
};

void shutdown_handler(int signal) {
  LOG(INFO) << "Shutting down simulation service...";
  exit(signal);
}

int main(int argc, char **argv) {
  gflags::ParseCommandLineFlags(&argc, &argv, true);
  signal(SIGINT, shutdown_handler);
  signal(SIGTERM, shutdown_handler);

  SimulationServiceImpl service(FLAGS_threads);

  ServerBuilder builder;
  const std::string address = absl::StrCat(FLAGS_host, ":", FLAGS_port);
  builder.AddListeningPort(address, grpc::InsecureServerCredentials());
  builder.RegisterService(&service);

  std::unique_ptr<Server> server(builder.BuildAndStart());
  LOG(INFO) << "Simulation server listening at " << address;
  LOG(INFO) << "Engine running with " << FLAGS_threads << " threads.";
  server->Wait();
  return 0;
}
