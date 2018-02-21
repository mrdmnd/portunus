#include <grpc++/grpc++.h>
#include <iostream>
#include <memory>
#include <string>

#include "simulate/proto/simulation_service.grpc.pb.h"

#include "absl/strings/str_cat.h"
#include "gflags/gflags.h"

DEFINE_string(host, "localhost", "Which port to request on.");
DEFINE_string(port, "50051", "Which port to request on.");

using grpc::Channel;
using grpc::ClientContext;
using grpc::Status;
using simulate::EncounterConfig;
using simulate::EquipmentConfig;
using simulate::GraphDef;
using simulate::SimulationConfig;
using simulate::SimulationRequest;
using simulate::SimulationResponse;
using simulate::SimulationService;
using simulate::TalentConfig;
using simulate::TrainedPolicy;
using simulate::WearableItem;

class SimulationServiceClient {
public:
  SimulationServiceClient(std::shared_ptr<Channel> channel)
      : stub_(SimulationService::NewStub(channel)) {}

  std::string IssueQuery(const std::string &name) {
    /*
        GraphDef *graph = new GraphDef;
        graph->set_content("lol");

        TrainedPolicy *policy = new TrainedPolicy;
        policy->set_allocated_tensorflow_graph(graph);

        WearableItem *hat = new WearableItem;
        hat->set_id(0);
        hat->set_name("giggle hat");

        EquipmentConfig *equipment = new EquipmentConfig;
        equipment->set_allocated_head(hat);

        TalentConfig *talents = new TalentConfig;
        talents->set_row_1(1);
        talents->set_row_2(2);
        talents->set_row_3(3);
        talents->set_row_4(2);
        talents->set_row_5(1);
        talents->set_row_6(2);
        talents->set_row_7(3);

        EncounterConfig *encounter = new EncounterConfig;
        encounter->set_id(69);
        encounter->set_name("Patchwork");

        SimulationConfig *sim_config = new SimulationConfig;
        sim_config->set_allocated_policy(policy);
        sim_config->set_allocated_equipment_config(equipment);
        sim_config->set_allocated_talent_config(talents);
        sim_config->set_allocated_encounter_config(encounter);

        SimulationRequest *request = new SimulationRequest;
        request->set_allocated_config(sim_config);

    */
    SimulationRequest request;
    request.set_request_string("MARIO");
    SimulationResponse response;

    // Special, client specific metadata that can be used to convey extra
    // information to the server or tweak RPC behavior.
    ClientContext context;

    Status status = stub_->ConductSimulation(&context, request, &response);
    if (status.ok()) {
      return response.response_string();
    } else {
      std::cout << status.error_code() << ": " << status.error_message()
                << std::endl;
      return "RPC Failed.";
    }
  }

private:
  std::unique_ptr<SimulationService::Stub> stub_;
};

int main(int argc, char **argv) {
  gflags::ParseCommandLineFlags(&argc, &argv, true);

  const auto target = absl::StrCat(FLAGS_host, ":", FLAGS_port);
  const auto credentials = grpc::InsecureChannelCredentials();
  const auto channel = grpc::CreateChannel(target, credentials);
  SimulationServiceClient client(channel);

  std::string name("doesn't matter");
  std::string reply = client.IssueQuery(name);
  std::cout << "Received reply: " << reply << std::endl;
}
