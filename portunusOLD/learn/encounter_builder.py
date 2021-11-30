""" This module provides an interface to build up encounter_config objects. """
from proto import encounter_config_pb2
from google.protobuf.text_format import Merge


class EncounterBuilder:
    """ Somewhat useless core example. """

    def __init__(self):
        self.encounter_config = encounter_config_pb2.EncounterConfig()

    def Build(self):
        return self.encounter_config


class Textproto(EncounterBuilder):
    """ The trivial implementation. Load a pre-defined textproto file. """

    def __init__(self, filepath):
        EncounterBuilder.__init__(self)
        with open(filepath, 'r') as f:
            Merge(f.read(), self.encounter_config)


class WarcraftLogs(EncounterBuilder):
    """ A fancy / smart implementation that parses WCL data for events. """

    def __init__(self, url):
        EncounterBuilder.__init__(self)
        # TODO(mrdmnd) - think up some interface for building an encounter from
        # a bunch of warcraft logs files.
