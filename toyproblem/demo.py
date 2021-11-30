import random
import collections
import math

from mcts import MCTS, Node

# Imagine a toy problem: find the optimal rotation for a (GREATLY SIMPLIFIED) model of an outlaw rogue.
# Our rogue has very small state: combo points (0 - 5), energy (0 - 100), and potentially an opportunity buff (see later).
# Every GCD, the rogue recovers 14 energy. By default, set the GCD to 1s.
# The rogue has three abilities (and four actions, when we count the "wait/pool" action).
# - Sinister Strike: Costs 45 energy, generates 1 combo point, does 45 damage.
#   - Has a 35% chance to strike twice (dealing 90 total damage, generating 2 combo points).
#   - If this proc happens, give the player 10s of an "opportunity buff" that makes the next use of Pistol Shot cost 50% energy and deal double damage.
# - Pistol Shot: Costs 40 energy, generates 1 combo point, does 35 damage.
#   - If opportunity buff is up, costs 20 energy and does 70 damage.
# - Dispatch: Costs 35 energy. Consumes all CP, though you must have at least one. Does CP * 35 damage.
# - "Wait": The rogue does nothing; energy pools up during this time.

INITIAL_TARGET_HP = 500

# We define our three abilities (four actions) here:
class Wait100MS:
    def __repr__(self):
        return "Wait100MS()"

    def Usable(self, state):
        return True

    # Returns a new state after executing this action.
    def Execute(self, state):
        new_energy = min(100, state.energy + 1.4)
        new_opportunity_buff_remains = max(0, state.opportunity_buff_remains - 0.1)
        new_elapsed_time = state.elapsed_time + 0.1
        return State(state.cp, new_energy, new_opportunity_buff_remains, state.target_hp, new_elapsed_time)

class SinisterStrike:
    def __repr__(self):
        return "SinisterStrike()"

    def Usable(self, state):
        return state.energy >= 45

    # Precondition: this action already is deemed legal to use.
    # Returns a new state after executing this action.
    def Execute(self, state):
        double_proc = random.random() < 0.35
        damage = 45
        cp_delta = 1
        energy_delta = -45

        new_opportunity_buff_remains = state.opportunity_buff_remains
        if double_proc:
            new_opportunity_buff_remains = 10
            damage *= 2
            cp_delta *= 2

        new_cp = min(5, state.cp + cp_delta)
        new_energy = state.energy + energy_delta
        new_target_hp = state.target_hp - damage

        # Now play forward the GCD until the next action:
        new_energy = min(100, new_energy+14)
        new_opportunity_buff_remains = max(0, new_opportunity_buff_remains - 1)
        new_elapsed_time = state.elapsed_time + 1.0

        return State(new_cp, new_energy, new_opportunity_buff_remains, new_target_hp, new_elapsed_time)

class PistolShot:
    def __repr__(self):
        return "PistolShot()"

    def Usable(self, state):
        return state.energy >= (40 - 20*(state.opportunity_buff_remains > 0))

    # Precondition: this action already is deemed legal to use.
    # Returns a new state after executing this action.
    def Execute(self, state):
        damage = 35
        cp_delta = 1
        energy_delta = -40
        if state.opportunity_buff_remains:
            damage *- 2
            energy_delta /= 2

        new_cp = min(5, state.cp + cp_delta)
        new_energy = state.energy + energy_delta
        new_opportunity_buff_remains = 0
        new_target_hp = state.target_hp - damage

        # Play forward the GCD until the next action.
        new_energy = min(100, new_energy+14)
        new_opportunity_buff_remains = max(0, new_opportunity_buff_remains - 1)
        new_elapsed_time = state.elapsed_time + 1.0

        return State(new_cp, new_energy, new_opportunity_buff_remains, new_target_hp, new_elapsed_time)


class Dispatch:
    def __repr__(self):
        return "Dispatch()"

    def Usable(self, state):
        return state.energy >= 35 and state.cp > 0

    # Precondition: this action already is deemed legal to use.
    def Execute(self, state):
        damage = 35 * state.cp
        energy_delta = -35

        new_cp = 0
        new_energy = state.energy + energy_delta
        new_target_hp = state.target_hp - damage

        # Play forward the GCD until the next action.
        new_energy = min(100, new_energy+14)
        new_opportunity_buff_remains = max(0, state.opportunity_buff_remains - 1)
        new_elapsed_time = state.elapsed_time + 1.0

        return State(new_cp, new_energy, new_opportunity_buff_remains, new_target_hp, new_elapsed_time)



ACTIONS = [Wait100MS(), SinisterStrike(), PistolShot(), Dispatch()]

class State(Node):
    def __init__(self, cp, energy, opportunity_buff_remains, target_hp, elapsed_time):
        self.cp = cp
        self.energy = energy
        self.opportunity_buff_remains = opportunity_buff_remains
        self.target_hp = target_hp
        self.elapsed_time = elapsed_time # NOT PART OF HASH OR EQUALITY! Necessary for markov (time independence) prop

    def FindChildren(self):
        if self.IsTerminal():
            return []
        return [action.Execute(self) for action in ACTIONS if action.Usable(self)]

    def IsTerminal(self):
        return self.target_hp < 0
    
    def Reward(self):
        return INITIAL_TARGET_HP / self.elapsed_time
    
    def __hash__(self):
        return hash(self.cp) ^ hash(self.energy) ^ hash(self.opportunity_buff_remains) ^ hash(self.target_hp)

    def __eq__(node1, node2):
        return node1.cp == node2.cp and \
               node1.energy == node2.energy and \
               node1.opportunity_buff_remains == node2.opportunity_buff_remains and \
               node1.target_hp == node2.target_hp

    def __repr__(self):
        return "State(cp={0}, energy={1}, opportunity={2}, hp_remaining={3}, elapsed_time={4})".format(self.cp, round(self.energy, 2), round(self.opportunity_buff_remains, 2), self.target_hp, round(self.elapsed_time, 2))



def Solve():
    tree = MCTS()
    state = State(0, 100.0, 0.0, INITIAL_TARGET_HP, 0.0)
    while not state.IsTerminal():
        for _ in range(100):
            tree.DoRollout(state)
        print("Best successor state from state {0} is {1}".format(state, tree.Choose(state)))
        state = tree.Choose(state)

if __name__ == "__main__":
    Solve()
