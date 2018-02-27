from __future__ import absolute_import
from __future__ import division
from __future__ import print_function

import sys

import grpc

from absl import app
from absl import flags
from absl import logging
from google.protobuf.text_format import MessageToString
from google.protobuf.text_format import Merge

from proto import encounter_config_pb2
from proto import player_config_pb2
from proto import policy_pb2
from proto import simulation_pb2
from proto import service_pb2
from proto import service_pb2_grpc

FLAGS = flags.FLAGS
flags.DEFINE_bool('use_gpu', False, 'Use the GPU for deep learning.')
flags.DEFINE_string('host', 'localhost', 'Hostname of simulation service.')
flags.DEFINE_string('port', '50051', 'Port of simulation service.')

def read_textproto_path(filepath, message):
  with open(filepath, 'r') as f:
    Merge(f.contents(), message)

def main(argv):
  encounter_path = argv[1]
  logging.info('Running under Python {0[0]}.{0[1]}.{0[2]}'.format(sys.version_info), file=sys.stderr)
  logging.info('Use GPU for deep learning: %s' % str(FLAGS.use_gpu))
  logging.info('Encounter file: %s' % encounter_path)
  #channel = grpc.insecure_channel(HOST + ":" + PORT)
  #simulation_stub = service_pb2_grpc.SimulationServiceStub(channel)

  encounter_config = encounter_config_pb2.EncounterConfig()
  read_textproto_path(encounter_path, encounter_config)
  print(MessageToString(encounter_config))


if __name__ == '__main__':
  app.run(main)
