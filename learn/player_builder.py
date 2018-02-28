""" This module provides an interface to build up player_config objects. """
from proto import player_config_pb2
from proto import gearset_pb2
import gearset_summarizer


class PlayerBuilder:
    """ Somewhat useless core example. """

    def __init__(self):
        self.player_config = player_config_pb2.PlayerConfig()

    def Build(self):
        return self.player_config


class SimcReport(PlayerBuilder):
    CLASS_WHITELIST = [
        "rogue=", "mage=", "warlock=", "warrior=", "death_knight=", "paladin=",
        "hunter=", "priest=", "shaman=", "monk=", "druid=", "demon_hunter="
    ]

    SLOT_WHITELIST = [
        "head=", "neck=", "shoulder=", "back=", "chest=", "wrist=", "legs=",
        "feet=", "finger=", "finger1=", "finger2=", "trinket=", "trinket1=",
        "trinket2=", "main_hand=", "off_hand="
    ]

    HEADER_WHITELIST = ["race=", "talents=", "spec="]

    WHITELIST = CLASS_WHITELIST + SLOT_WHITELIST + HEADER_WHITELIST

    def __init__(self, filepath):
        PlayerBuilder.__init__(self)
        (header, gear, bags) = self.__Partition(filepath)
        self.__SetPlayerConfigMetadata(header)
        self.__SetPlayerConfigEquipmentSummary(gear)

        # TODO(mrdmnd) - implement these methods, and then a pruning pass.
        # An dictionary mapping each slot to a single piece of gear.
        # self.current_gear_dict = self.__BuildGear(gear)
        # A dictionary mapping each slot to a list of pieces for that slot.
        # self.potential_gear_dict = self.__BuildGear(gear + bags)

    def __Partition(self, filepath):
        """ Splits report file into groups: header, current_gear, bag_gear. """
        with open(filepath) as f:
            lines = [line.strip() for line in f.readlines()]
            filtered = [
                l for l in lines if any(w in l for w in self.WHITELIST)
            ]
            bags = [line[2:] for line in filtered if line[0] is '#']
            non_bags = [line for line in filtered if line[0] is not '#']
            start_of_gear = len(self.HEADER_WHITELIST) + 1
            header = non_bags[:start_of_gear]
            gear = non_bags[start_of_gear:]
        return (header, gear, bags)

    def __SetPlayerConfigMetadata(self, header):
        """ Sets up base metadata for player. """
        for line in header:
            (k, v) = line.split("=")
            if k == "talents":
                talent_string = v
            elif k == "race":
                race_string = v.upper()
            elif k == "spec":
                spec_string = v.upper()
            else:
                class_string = k.upper()
                name_string = v
        self.player_config.name = name_string[1:-1]
        self.player_config.race = player_config_pb2.Race.Value(race_string)
        self.player_config.talents.row_1 = int(talent_string[0])
        self.player_config.talents.row_2 = int(talent_string[1])
        self.player_config.talents.row_3 = int(talent_string[2])
        self.player_config.talents.row_4 = int(talent_string[3])
        self.player_config.talents.row_5 = int(talent_string[4])
        self.player_config.talents.row_6 = int(talent_string[5])
        self.player_config.talents.row_7 = int(talent_string[6])
        self.player_config.spec = player_config_pb2.Specialization.Value(
            class_string + "_" + spec_string)

    def __SetPlayerConfigEquipmentSummary(self, gear):
        """ Builds a gearset from `gear` lines, then runs GearsetSummarizer. """
        gearset = gearset_pb2.Gearset()
        for line in gear:
            item = gearset_pb2.WearableItem()
            comma_splits = line.split(",")
            for partition in comma_splits:
                (k, v) = partition.split("=")
                if k == "id":
                    item.item_id = int(v)
                elif k == "enchant_id":
                    item.enchant_id = int(v)
                elif k == "gem_id":
                    item.gem_ids.extend([int(g_id) for g_id in v.split("/")])
                elif k == "bonus_id":
                    item.bonus_ids.extend([int(b_id) for b_id in v.split("/")])
                elif k != "relic_id":
                    item.slot = gearset_pb2.Slot.Value(k.upper())
            gearset.items.extend([item])
        equipment_summary = gearset_summarizer.SummarizeGearset(gearset)
        self.player_config.equipment_summary.MergeFrom(equipment_summary)
