use portunus::simulation_service_client::SimulationServiceClient;
use portunus::SimulationRequest;

pub mod portunus {
    tonic::include_proto!("portunus");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = SimulationServiceClient::connect("http://[::1]:50051").await?;
    let request = tonic::Request::new(SimulationRequest {config: "foobar".into()});
    let response = client.conduct_simulation(request).await?;
    println!("RESPONSE={:?}", response);
    Ok(())
}

