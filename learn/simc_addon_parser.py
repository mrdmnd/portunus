""" This library is meant to provide an interface to parse SIMC addon textual 
    output into the protocol buffers that our policygen program needs."""

from proto import player_config_pb2

class SimcAddonReport:
    def __init__(self, filepath):
        """ On construction, parse file at filepath. Contents should be a
            dictionary containing relevant protobuf bits."""
        self.contents_ = self.__ParseSimcAddonFile(filepath)

    def __ParseSimcAddonFile(self, filepath):
        return None

    def GetPlayerConfig(self):
        """ Returns a PlayerConfig protobuf corresponding to the current gearset
            equipped by the player. """
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

        return player_config

    def GetAllWearableItemsFromFile(self):
        return None

