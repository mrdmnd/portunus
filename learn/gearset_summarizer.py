from proto import player_config_pb2
from proto import gearset_pb2


def SummarizeGearset(gearset):
    """ Converts a Gearset proto into an EquipmentSummary proto. """
    equipment_summary = player_config_pb2.EquipmentSummary()
    return equipment_summary
