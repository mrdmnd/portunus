from __future__ import absolute_import
from __future__ import division
from __future__ import print_function

import sys
import os
import grpc
import tensorflow as tf

from absl import app
from absl import flags
from absl import logging

from google.protobuf.text_format import MessageToString

from proto import policy_pb2
from proto import simulation_pb2
from proto import service_pb2
from proto import service_pb2_grpc

import encounter_builder
import player_builder

FLAGS = flags.FLAGS
flags.DEFINE_bool('use_gpu', False, 'Use the GPU for deep learning.')
flags.DEFINE_string('host', 'localhost', '')
flags.DEFINE_string('port', '50051', '')
flags.DEFINE_integer('sim_max_iterations', 10000, '')
flags.DEFINE_float('sim_target_error', 0.01, '')


def main(argv):
    # Sanity checking our environment.
    logging.info('Running Python {0[0]}.{0[1]}.{0[2]}'.format(
        sys.version_info))
    logging.info('Policygen should use GPU for deep learning: {0}'.format(
        FLAGS.use_gpu))
    logging.info('Tensorflow is built with CUDA support: {0}'.format(
        tf.test.is_built_with_cuda()))
    logging.info('Tensorflow has CUDA-supporting GPU available: {0}'.format(
        tf.test.is_gpu_available(cuda_only=True)))

    # Ensure parseable configs for encounter and player exist.
    if len(argv) != 3:
        logging.error("You must pass in an encounter and a player file.")
        return 1

    # Try to setup communication with RPC simulation service.
    channel = grpc.insecure_channel(FLAGS.host + ":" + FLAGS.port)
    stub = service_pb2_grpc.SimulationServiceStub(channel)

    # Setup factory for encounter and player configs.
    encounter_path = argv[1]
    encounter_extension = os.path.splitext(encounter_path)[1]
    if encounter_extension == ".textproto":
        encounter_factory = encounter_builder.Textproto(encounter_path)
    else:
        logging.error("I don't know how to parse that kind of encounter file.")
        return 1

    player_path = argv[2]
    player_extension = os.path.splitext(player_path)[1]
    if player_extension == ".textproto":
        player_factory = player_builder.Textproto(player_path)
    elif player_extension == ".simcdump":
        player_factory = player_builder.SimcReport(player_path)
    else:
        logging.error("I don't know how to parse that kind of player file.")
        return 1

    # Load encounter and player configurations.
    encounter_config = encounter_factory.Build()
    logging.info("EncounterConfig\n" + MessageToString(encounter_config))
    player_config = player_factory.Build()
    logging.info("PlayerConfig:\n" + MessageToString(player_config))

    # Const part of simulation configuration (encounter, player, static params)
    fixed_simulation_config = simulation_pb2.SimulationConfig()
    fixed_simulation_config.max_iterations = FLAGS.sim_max_iterations
    fixed_simulation_config.target_error = FLAGS.sim_target_error
    fixed_simulation_config.encounter_config.MergeFrom(encounter_config)
    fixed_simulation_config.player_config.MergeFrom(player_config)

    # Training loop:
    converged = False
    while not converged:
        # Policy update
        policy = policy_pb2.Policy()
        policy.contents = "POLICY GRAPH"

        # "Variable" part of simulation configuration
        simulation_config = simulation_pb2.SimulationConfig()
        simulation_config.CopyFrom(fixed_simulation_config)
        simulation_config.policy.MergeFrom(policy)

        # Issue request to RPC stub.
        request = service_pb2.SimulationRequest()
        request.config.MergeFrom(simulation_config)
        response = stub.ConductSimulation(request)
        logging.info("Simulation response:\n" + MessageToString(response))

        # TODO(mrdmnd) - For initial purposes, just do this once.
        converged = True
    return 0


if __name__ == '__main__':
    app.run(main)
