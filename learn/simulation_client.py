""" A simple class for communicating with a simulation gRPC service. """
import grpc
from google.protobuf.text_format import MessageToString

from proto import actor_pb2
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
    enemy_actor = actor_pb2.Actor()
    enemy_actor.id = 1
    enemy_actor.name = "Patchwork"
    enemy_actor.hp_max = 1000
    enemy_actor.hp_current = 1000

    spawn_event = encounter_pb2.ActorSpawnEvent()
    spawn_event.actor.MergeFrom(enemy_actor)
    spawn_event.despawn_timestamp = -1

    encounter_event = encounter_pb2.EncounterEvent()
    encounter_event.timestamp = -1
    encounter_event.spawn.MergeFrom(spawn_event)

    encounter_config = encounter_pb2.EncounterConfig()
    encounter_config.name = "PatchworkFight"
    encounter_config.events.extend([encounter_event])

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
    simulation_config.max_iterations = 10000
    simulation_config.error_target = 0.01
    simulation_config.time_target = 300
    simulation_config.time_variance = 0.05
    simulation_config.encounter_config.MergeFrom(encounter_config)
    simulation_config.equipment_config.MergeFrom(equipment_config)
    simulation_config.talent_config.MergeFrom(talent_config)
    simulation_config.policy.MergeFrom(policy)

    # Issue request to RPC stub.
    simulation_request = service_pb2.SimulationRequest()
    simulation_request.config.MergeFrom(simulation_config)
    simulation_response = simulation_stub.ConductSimulation(simulation_request)

    output = MessageToString(simulation_response.result)
    print(output)

if __name__ == '__main__':
    main()
