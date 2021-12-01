import random
import collections
import math

class State:
    def __init__(self, target_health=1000, energy=100, combo_points=0, gcd_remains=0.0, opportunity_remains=0.0, sim_time=0.0):
        self.target_health = target_health
        self.energy = energy
        self.combo_points = combo_points
        self.gcd_remains = gcd_remains
        self.opportunity_remains = opportunity_remains
        self.sim_time = sim_time

    def __repr__(self):
        return "State(target_health={0}, combo_points={1}, energy={2}, gcd_remains={3}, opportunity_remains={4}, sim_time={5})" \
        .format(self.target_health, 
                self.combo_points, 
                round(self.energy, 2), 
                round(self.gcd_remains, 2), 
                round(self.opportunity_remains, 2), 
                round(self.sim_time, 2))

    # Returns true if combat is over.
    def IsTerminal(self):
        return self.target_health <= 0

    # Applies the action to this state, returns a probability distribution.
    def PerformAction(self, action):
        return action.OutcomeDistribution(self)

    def LegalActions(self):
        return [action for action in ACTIONS if action.UsableFromState(self)]
        

# Abstract Base class for implementing actions that modify states.
class Action:
    # Returns true if the given action is usable from the given state.
    def UsableFromState(start_state):
        pass
   
    # Precondition: this action already is deemed legal to use.
    # Returns a dictionary representing a probability distribution over potential new states.
    # For each possible state you could end up in, that's a key.
    # Associated with that key is the probability of ending up in that state.
    def OutcomeDistribution(start_state):
        pass

# We define our three abilities (four actions) here:
###############################################################################

class Wait(Action):
    def __repr__(self):
        return "Wait"

    # You can always wait 100ms in our toy problem.
    def UsableFromState(start_state):
        return True

    # In our problem, waiting 100ms generates some energy regeneration and reduces the GCD timer + opportunity buff duration.
    def OutcomeDistribution(start_state):
        outcomes = {}

        new_target_health = start_state.target_health
        new_energy = min(100, start_state.energy + 1.4)
        new_combo_points = start_state.combo_points
        new_gcd_remains = max(0, start_state.gcd_remains - 0.1)
        new_opportunity_remains = max(0, start_state.opportunity_remains - 0.1)
        new_sim_time = start_state.sim_time + 0.1

        outcome_state = State(new_target_health, new_energy, new_combo_points, new_gcd_remains, new_opportunity_remains, new_sim_time)
        outcomes[outcome_state] = 1.0


        return outcomes

class SinisterStrike(Action):
    def __repr__(self):
        return "SinisterStrike"

    def UsableFromState(start_state):
        return start_state.energy >= 45 and start_state.gcd_remains == 0.0

    def OutcomeDistribution(start_state):
        outcomes = {}

        damage = 45
        cp_delta = 1
        energy_delta = -45

        new_energy = min(100, start_state.energy + energy_delta)
        new_gcd_remains = 1.0
        new_sim_time = start_state.sim_time

        # Case 1) No double proc (happens 65% of the time)
        new_target_health = max(0, start_state.target_health - damage)
        new_combo_points = min(5, start_state.combo_points + cp_delta)
        new_opportunity_remains = start_state.opportunity_remains
        no_proc = State(new_target_health, new_energy, new_combo_points, new_gcd_remains, new_opportunity_remains, new_sim_time)
        outcomes[no_proc] = 0.65

        # Case 2) Double proc (happens 35% of the time)
        new_target_health = max(0, start_state.target_health - 2*damage)
        new_combo_points = min(5, start_state.combo_points + 2*cp_delta)
        new_opportunity_remains = 10.0
        proc = State(new_target_health, new_energy, new_combo_points, new_gcd_remains, new_opportunity_remains, new_sim_time)
        outcomes[proc] = 0.35
        
        return outcomes

class PistolShot(Action):
    def __repr__(self):
        return "PistolShot"

    def UsableFromState(start_state):
        return (start_state.energy >= 40 or (start_state.energy >= 20 and start_state.opportunity_remains > 0)) and start_state.gcd_remains == 0.0

    def OutcomeDistribution(start_state):
        outcomes = {}
        # Two options: you either have opportunity buff up, or you do not.
        # This, however, is NOT stochastic; just a difference in logic:
        # The probability distribution over states after pistol shotting should have only one element in it.
        damage = 35
        cp_delta = 1
        energy_delta = -40
        if start_state.opportunity_remains:
            damage *= 2
            energy_delta /= 2


        new_target_health = max(0, start_state.target_health - damage)
        new_energy = min(100, start_state.energy + energy_delta)
        new_combo_points = min(5, start_state.combo_points + cp_delta)
        new_gcd_remains = 1.0
        new_opportunity_remains = 0.0
        new_sim_time = start_state.sim_time

        outcome_state = State(new_target_health, new_energy, new_combo_points, new_gcd_remains, new_opportunity_remains, new_sim_time)
        outcomes[outcome_state] = 1.0

        return outcomes

class Dispatch(Action):
    def __repr__(self):
        return "Dispatch"

    def UsableFromState(start_state):
        return start_state.energy >= 35 and start_state.combo_points > 0 and start_state.gcd_remains == 0.0

    def OutcomeDistribution(start_state):
        outcomes = {}

        damage = 35 * start_state.combo_points
        energy_delta = -35

        new_target_health = max(0, start_state.target_health - damage)
        new_energy = min(100, start_state.energy + energy_delta)
        new_combo_points = 0
        new_gcd_remains = 1.0
        new_opportunity_remains = start_state.opportunity_remains
        new_sim_time = start_state.sim_time

        outcome_state = State(new_target_health, new_energy, new_combo_points, new_gcd_remains, new_opportunity_remains, new_sim_time)
        outcomes[outcome_state] = 1.0
        return outcomes

ACTIONS = [Wait, SinisterStrike, PistolShot, Dispatch]