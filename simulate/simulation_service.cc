#include <iostream>
#include <memory>
#include <string>

#include "simulate/proto/simulation_service.grpc.pb.h"

#include <grpc++/grpc++.h>

using grpc::Server;
using grpc::ServerBuilder;
using grpc::ServerContext;
using grpc::Status;

class SimulationServiceImpl final
    : public simulate::SimulationService::Service {
  Status SimulationRequest(ServerContext *context,
                           const simulate::SimulationRequest *request,
                           simulate::SimulationResponse *response) {
    std::string hi("Hello.");
    response->set_message(hi);
    return Status::OK;
  }
};

void RunServer() {
  std::string address("0.0.0.0:50051");

  SimulationServiceImpl service;
  ServerBuilder builder;
  builder.AddListeningPort(address, grpc::InsecureServerCredentials());
  builder.RegisterService(&service);
  std::unique_ptr<Server> server(builder.BuildAndStart());
  std::cout << "Simulation service listening on " << address << std::endl;
  server->Wait();
}

int main() {
  RunServer();
  return 0;
}
