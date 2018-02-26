""" A simple class for communicating with a simulation gRPC service. """
import grpc
from google.protobuf.text_format import MessageToString
from google.protobuf.text_format import Merge

from proto import encounter_config_pb2
from proto import player_config_pb2
from proto import policy_pb2
from proto import simulation_pb2
from proto import service_pb2
from proto import service_pb2_grpc

HOST = "localhost"
PORT = "50051"


def main():
  """ Entry point into simulation client. """
  channel = grpc.insecure_channel(HOST + ":" + PORT)
  simulation_stub = service_pb2_grpc.SimulationServiceStub(channel)

  # Encounter configuration
  spawn_event = encounter_config_pb2.EncounterEvent()
  spawn_event.timestamp = -1  # A value of -1 means "right before combat"
  spawn_event.spawn.MergeFrom(encounter_config_pb2.EnemySpawnEvent())
  spawn_event.spawn.enemy.MergeFrom(encounter_config_pb2.Enemy())
  spawn_event.spawn.enemy.id = 1
  spawn_event.spawn.enemy.name = "Patchwork"
  spawn_event.spawn.enemy.type = encounter_config_pb2.EnemyType.Value("BOSS")

  bloodlust_event = encounter_config_pb2.EncounterEvent()
  bloodlust_event.timestamp = 500  # 500ms into the fight
  bloodlust_event.lust.MergeFrom(encounter_config_pb2.BloodlustEvent())
  bloodlust_event.lust.duration = 30000  # 30s
  bloodlust_event.lust.haste_percentage = 0.3

  encounter_config = encounter_config_pb2.EncounterConfig()
  encounter_config.name = "PatchworkFight"
  encounter_config.events.extend([spawn_event, bloodlust_event])
  encounter_config.health_estimator = encounter_config_pb2.HealthEstimator.Value(
      "BURST_AND_EXECUTE")
  encounter_config.time_target = 300000  # 300s
  encounter_config.time_variance = 0.05

  # Player configuration
  player_config = player_config_pb2.PlayerConfig()
  player_config.equipment_config.MergeFrom(player_config_pb2.EquipmentConfig())
  player_config.talent_config.MergeFrom(player_config_pb2.TalentConfig())
  player_config.talent_config.row_1 = 1
  player_config.talent_config.row_2 = 3
  player_config.talent_config.row_3 = 3
  player_config.talent_config.row_4 = 1
  player_config.talent_config.row_5 = 2
  player_config.talent_config.row_6 = 3
  player_config.talent_config.row_7 = 1

  # Policy configuration
  policy = policy_pb2.Policy()
  policy.contents = "POLICY GRAPH"

  # Simulation configuration
  simulation_config = simulation_pb2.SimulationConfig()
  simulation_config.max_iterations = 10000
  simulation_config.target_error = 0.01
  simulation_config.encounter_config.MergeFrom(encounter_config)
  simulation_config.player_config.MergeFrom(player_config)
  simulation_config.policy.MergeFrom(policy)

  # Issue request to RPC stub.
  simulation_request = service_pb2.SimulationRequest()
  simulation_request.config.MergeFrom(simulation_config)
  simulation_response = simulation_stub.ConductSimulation(simulation_request)

  output = MessageToString(simulation_response.result)
  print(output)


if __name__ == "__main__":
  main()
