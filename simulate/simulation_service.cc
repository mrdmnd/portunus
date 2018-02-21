#include <iostream>
#include <memory>
#include <string>

#include "simulate/proto/simulation_service.grpc.pb.h"

#include <gflags/gflags.h>
#include <grpc++/grpc++.h>

DEFINE_string(port, "50051", "Which port to serve requests from.");

using grpc::Server;
using grpc::ServerBuilder;
using grpc::ServerCompletionQueue;
using grpc::ServerContext;
using grpc::Status;

class SimulationServiceImpl final
    : public simulate::SimulationService::Service {
  Status ConductSimulation(ServerContext *context,
                           const simulate::SimulationRequest *request,
                           simulate::SimulationResponse *response) override {
    std::string hi("Hello.");
    response->set_response_string(hi);
    return Status::OK;
  }
};

void RunServer(const std::string &address) {
  SimulationServiceImpl service;
  ServerBuilder builder;

  // Tell our factor what port to listen on.
  builder.AddListeningPort(address, grpc::InsecureServerCredentials());

  // Set up our work queue.
  std::unique_ptr<ServerCompletionQueue> queue = builder.AddCompletionQueue();

  // Hook the service implementation into the builder.
  builder.RegisterService(&service);

  // Launch and store the server in a unique_ptr
  std::unique_ptr<Server> server(builder.BuildAndStart());
  std::cout << "Simulation service listening on " << address << std::endl;

  // Serve until killed.
  server->Wait();
}

int main(int argc, char **argv) {
  gflags::ParseCommandLineFlags(&argc, &argv, true);
  RunServer(FLAGS_port);
  return 0;
}
