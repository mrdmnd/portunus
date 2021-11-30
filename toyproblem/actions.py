"""
This module implements all of the actions usable by a rogue in various encounter states.
Notably, we also include the "wait" action and "retarget" actions.
"""

class Action:
    # Returns true if the given action is usable from the given state.
    def UsableFromState(self, encounter_state):
        pass
   
    # Returns a probability distribution as a dictionary:
    # For each possible state you could end up in, that's a key.
    # Associated with that key is the probability of ending up in that state.
    def NewStateDistribution(self, encounter_state):
        pass

    # Returns a probability distribution as a dictionary:
    # For each possible state you could end up in, that's a key.
    # Associated with that key is the immediate reward of ending up in that state.
    def ImmediateRewardDistribution(self, encounter_state):
        pass

###############################################################################
# 
#
#                           Action Definitions                                #
#
#
###############################################################################

# Default Actions
class Wait(Action):
    def UsableFromState(self, state):
        return True

class Retarget(Action):
    def __init__(self, enemy_index):
        pass

# Cooldowns
# Generic base class. Need to implement subclasses.
# Will start with my choices, because YOLO.
class RacialActive(Action):
    pass

class TrinketActive(Action):
    pass

class CovenantActive(Action):
    pass

# Specific Cooldowns
class AdrenalineRush(Action):
    pass

class Vanish(Action):
    pass

class RollTheBones(Action):
    pass

class BladeFlurry(Action):
    pass

# Talents
class GhostlyStrike(Action):
    pass

class MarkedForDeath(Action):
    pass

class Dreadblades(Action):
    pass

class BladeRush(Action):
    pass

class KillingSpree(Action):
    pass

# Core Abilities
class Ambush(Action):
    pass

class SinisterStrike(Action):
    pass

class PistolShot(Action):
    pass

class BetweenTheEyes(Action):
    pass

class Dispatch(Action):
    pass

class SliceAndDice(Action):
    pass


ALL_ACTIONS = [Wait, Retarget, RacialActive, TrinketActive, CovenantActive, AdrenalineRush, Vanish, RollTheBones, BladeFlurry, \
        GhostlyStrike, MarkedForDeath, Dreadblades, BladeRush, KillingSpree, Ambush, SinisterStrike, PistolShot, BetweenTheEyes, \
        Dispatch, SliceAndDice]

