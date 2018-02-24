#include <iostream>
#include <memory>
#include <string>

#include "proto/simulation_service.grpc.pb.h"

#include "absl/strings/str_cat.h"
#include "gflags/gflags.h"
#include "glog/logging.h"
#include "grpc++/grpc++.h"
#include "grpc/support/log.h"

DEFINE_string(host, "localhost", "Which address to serve from.");
DEFINE_string(port, "50051", "Which port to serve requests from.");

using grpc::Server;
using grpc::ServerAsyncResponseWriter;
using grpc::ServerBuilder;
using grpc::ServerCompletionQueue;
using grpc::ServerContext;
using grpc::Status;

enum class CallStatus { CREATE, PROCESS, FINISH };

class SimulationServiceImpl final {
public:
  ~SimulationServiceImpl() {
    LOG(INFO) << "Simulation service shutting down.";
    server_->Shutdown();
    cq_->Shutdown();
  }

  void RunServer(const std::string &address) {
    ServerBuilder builder;
    builder.AddListeningPort(address, grpc::InsecureServerCredentials());
    builder.RegisterService(&service_);
    cq_ = builder.AddCompletionQueue();
    server_ = builder.BuildAndStart();
    LOG(INFO) << "Simulation service listening on " << address;
    HandleRpcs();
  }

private:
  //  Set up a class with state and logic necessary to serve a request.
  class CallData {
  public:
    CallData(simulate::SimulationService::AsyncService *service,
             ServerCompletionQueue *cq)
        : service_(service), cq_(cq), responder_(&ctx_),
          status_(CallStatus::CREATE) {
      // Immediate start the serving logic in this constructor.
      Proceed();
    }

    void Proceed() {
      if (status_ == CallStatus::CREATE) {
        status_ = CallStatus::PROCESS;
        service_->RequestConductSimulation(&ctx_, &request_, &responder_, cq_,
                                           cq_, this);
      } else if (status_ == CallStatus::PROCESS) {
        new CallData(service_, cq_);
        LOG(INFO) << "Simulation service saw new RPC request.";
        // The actual processing:
        std::string hi("Hello.");
        response_.set_response_string(hi);

        // And we're done! Let gRPC runtime know that we're done, using the
        // memory address of this instance as the unique tag for the event.
        status_ = CallStatus::FINISH;
        responder_.Finish(response_, Status::OK, this);
      } else {
        GPR_ASSERT(status_ == CallStatus::FINISH);
        // Once in the finish state, deallocate ourselves (CallData).
        delete this;
      }
    }

  private:
    simulate::SimulationService::AsyncService *service_;
    ServerCompletionQueue *cq_;
    ServerContext ctx_;
    simulate::SimulationRequest request_;
    simulate::SimulationResponse response_;
    ServerAsyncResponseWriter<simulate::SimulationResponse> responder_;
    CallStatus status_;
  };

  void HandleRpcs() {
    new CallData(&service_, cq_.get());
    void *tag;
    bool ok;
    while (true) {
      // Block waiting to read the next event from the completion queue.. Event
      // is uniquely identified by its tag, which is the memory location of the
      // CallData instance associated with it.
      GPR_ASSERT(cq_->Next(&tag, &ok));
      GPR_ASSERT(ok);
      static_cast<CallData *>(tag)->Proceed();
    }
  }

  std::unique_ptr<ServerCompletionQueue> cq_;
  simulate::SimulationService::AsyncService service_;
  std::unique_ptr<Server> server_;
};

int main(int argc, char **argv) {
  gflags::ParseCommandLineFlags(&argc, &argv, true);
  SimulationServiceImpl server;
  server.RunServer(absl::StrCat(FLAGS_host, ":", FLAGS_port));
  return 0;
}
