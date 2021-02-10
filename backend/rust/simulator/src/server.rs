use tonic::{transport::Server, Request, Response, Status};

use portunus::simulation_service_server::{SimulationService, SimulationServiceServer};
use portunus::{SimulationRequest, SimulationResponse};

pub mod portunus {
    tonic::include_proto!("portunus");
}

#[derive(Debug, Default)]
pub struct MySimulationService {}

#[tonic::async_trait]
impl SimulationService for MySimulationService {
    async fn conduct_simulation(&self, request: Request<SimulationRequest>) -> Result<Response<SimulationResponse>, Status> {
        println!("Got a request: {:?}", request);
        let reply = portunus::SimulationResponse {result: format!("Hello {}!", request.into_inner().config).into()};
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Launching portunus socket server...");
    let addr = "[::1]:50051".parse()?;
    let simulation_service = MySimulationService::default();
    Server::builder().add_service(SimulationServiceServer::new(simulation_service)).serve(addr).await?;
    Ok(())
}