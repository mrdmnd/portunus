# Assumptions (tracked explicitly here).
# We assume you use instant poison.


# Need to track static configuration stuff here.

class StaticConfig:
    def __init__(self):
        # Passed-in Attributes
        self.race = race
        self.covenant = covenant
        self.soulbind = soulbind
        self.conduits = conduits
        self.talents = talents # Take as string, like 3121213 or something.
        self.enchants = enchants
        self.legendary_effects = legendary_effects

        # Computed/Derived/Constant Attributes
        self.global_cooldown_base = 1.0
        self.max_energy = self.max_energy

        self.base_agil_from_gear = 0
        self.base_crit_from_gear = 0
        self.base_vers_from_gear = 0
        self.base_mast_from_gear = 0
        self.base_hste_from_gear = 0

# Our Outlaw's State
class PlayerState:
    def __init__(self, static_config):
        self.static_config
        self.agility = 0

# Any given enemy state.
class EnemyState:
    def __init__(self):
        self.distance_from_player = distance_from_player
        self.health_remaining = health_remaining

# The overall state of our MDP!
# EnemyStates should be a list of enemies.
class EncounterState:
    def __init__(self, player_state, enemy_states):
        self.player_state = player_state
        self.enemy_states = enemy_states
