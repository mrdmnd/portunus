""" A simple class for communicating with a simulation gRPC service. """
import grpc
from google.protobuf.text_format import MessageToString

from proto import encounter_pb2
from proto import equipment_pb2
from proto import talents_pb2
from proto import policy_pb2
from proto import simulation_pb2
from proto import service_pb2
from proto import service_pb2_grpc

HOST = "localhost"
PORT = "50051"

def main():
    """ Entry point into simulation client. """
    channel = grpc.insecure_channel(HOST+":"+PORT)
    simulation_stub = service_pb2_grpc.SimulationServiceStub(channel)

    # Encounter configuration
    encounter_config = encounter_pb2.EncounterConfig()

    # Equipment configuration
    equipment_config = equipment_pb2.EquipmentConfig()

    # Talent configuration
    talent_config = talents_pb2.TalentConfig()
    talent_config.row_1 = 1;
    talent_config.row_2 = 3;
    talent_config.row_3 = 3;
    talent_config.row_4 = 1;
    talent_config.row_5 = 2;
    talent_config.row_6 = 3;
    talent_config.row_7 = 1;

    # Policy configuration
    policy = policy_pb2.PolicyConfig()
    policy.contents = "POLICY GRAPH"

    # Simulation configuration
    simulation_config = simulation_pb2.SimulationConfig()
    simulation_config.encounter_config.MergeFrom(encounter_config)
    simulation_config.equipment_config.MergeFrom(equipment_config)
    simulation_config.talent_config.MergeFrom(talent_config)
    simulation_config.policy.MergeFrom(policy)
    simulation_config.target_error = 0.01
    simulation_config.max_iterations = 10000
    simulation_config.combat_length_variance = 0.20

    simulation_request = service_pb2.SimulationRequest()
    simulation_request.config.MergeFrom(simulation_config)
    simulation_response = simulation_stub.ConductSimulation(simulation_request)

    output = MessageToString(simulation_response.result)
    print(output)

if __name__ == '__main__':
    main()
