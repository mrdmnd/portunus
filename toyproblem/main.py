import sys
import random

import sim

"""
A simple toy python example problem.
We model the encounter as a "state" in a markov decision process, and each action the player can take
results in a probability distribution over new encounter states.

We try to use open-loop MCTS to figure out the best policy in a given state.

Imagine a toy problem: find the optimal rotation for a (GREATLY SIMPLIFIED) model of an outlaw rogue.
Our rogue has very small state: combo points (0 - 5), energy (0 - 100), and potentially an "opportunity buff" (see later).
Every 1s of the simulation, the rogue recovers 14 energy. By default, we set our global cooldown to 1s (the tick duration).
The rogue has three abilities (and four actions, when we count the "wait/pool" action).
- Sinister Strike: Costs 45 energy, generates 1 combo point, does 45 damage.
  - Has a 35% chance to strike twice (dealing 90 total damage, generating 2 combo points).
  - If this proc happens, give the player 10s of an "opportunity buff" that makes the next use of Pistol Shot cost 50% energy and deal double damage.
- Pistol Shot: Costs 40 energy, generates 1 combo point, does 35 damage.
  - If opportunity buff is up, costs 20 energy and does 70 damage.
- Dispatch: Costs 35 energy. Consumes all CP, though you must have at least one. Does CP * 35 damage.
- "Wait": The rogue does nothing; energy pools up during this time. We default this to 100ms.

- The goal here is to learn a policy that performs an optimal rotation: use sinister strike to build combo points, and use opportunity procs when they are up.
"""

def weighted_sample(distribution):
    return random.choices(list(distribution.keys()), weights=distribution.values(), k=1)[0]

# Does a "random iteration", selecting a legal action uniformly at random from amongst the legal actions.
def UniformRandomPolicy(state):
    return random.choice(state.LegalActions())

# Executes a minimally intelligent policy.
def SmarterPolicy(state):
    if sim.Dispatch.UsableFromState(state) and state.combo_points >= 5:
        return sim.Dispatch
    elif sim.PistolShot.UsableFromState(state) and state.opportunity_remains > 0:
        return sim.PistolShot
    elif sim.SinisterStrike.UsableFromState(state) and state.combo_points < 5:
        return sim.SinisterStrike
    else:
        return sim.Wait
    

def iterate(start_state, policy):
    state = start_state
    #print("Starting iteration with state:")
    #print(state)
    while not state.IsTerminal():
        chosen_action = policy(state)
        #print(chosen_action)
        outcome_distribution = state.PerformAction(chosen_action)
        state = weighted_sample(outcome_distribution)
        #print(state)
    return state.sim_time

def main():
    n_iters = 100
    starting_state = sim.State(target_health=1000)

    uniform_random_average_fight_length = 0.0
    for i in range(n_iters):
        uniform_random_average_fight_length += iterate(starting_state, UniformRandomPolicy)

    smarter_average_fight_length = 0.0
    for i in range(n_iters):
        smarter_average_fight_length += iterate(starting_state, SmarterPolicy)

    print(uniform_random_average_fight_length / n_iters)
    print(smarter_average_fight_length / n_iters)


if __name__ == "__main__":
    main()
