import sys

from states import *
from actions import *

"""
A python script to do APL optimization via MARKOV DECISION PROCESS modelling.
We model the encounter as a "state" in a markov decision process, and each action the player can take
results in a probability distribution over new encounter states, as well as a probability distribution 
over immediate rewards.

We try to use MCTS to figure out the best policy in a given state.
"""

# Set this to less than one to value actions in the future slightly more than actions in the present
DISCOUNTING = 1.0

def StaticConfigFromSIMC(dump):
    key_list = ("race", "talents", "covenant")
    d = {}
    for line in dump:
        if line[0] == '#':
            continue
        for key in key_list:
            if line.startswith(key):
                k, v = line.split("=")
                d[k] = v.strip()
    
    # TODO(do gear lookup, get static stat allocation on it all)


def main(simc_dump_path):
    with open(simc_dump_path, 'r') as f:
        static_config = StaticConfigFromSIMC(f.readlines())
        print(static_config)

        outlaw_state = OutlawState()
        patchwork_state = EnemyState(3, 50000)
        enemy_states = [patchwork_state]

        encounter_state = EncounterState(outlaw_state, enemy_states)

        optimal_policy = SolveMDP(encounter_state, Actions.ALL_ACTIONS)


if __name__ == "__main__":
    simc_dump_path = sys.argv[1]
    main(simc_dump_path)
