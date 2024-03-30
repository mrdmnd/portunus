import numpy as np
from outlaw_environment import ACTIONS, Dispatch, PistolShot, SinisterStrike


class HumanController:
    def __init__(self):
        print("initializing human APL controller")
    
    # The core method for implementing the "SIMC APL" style decision tree.
    # Returns np.ndarray - the model's action.
    # We don't actually use the second parameter of the return though.
    def predict(self, observation, state=None, mask=None, deterministic=True):
        fth_stacks = observation[0][0]
        combo_points = observation[0][1]
        if combo_points >= 7 or (combo_points >= 6 and fth_stacks == 2):
            return np.array([ACTIONS.index(Dispatch)])
        elif (combo_points <= 3 and fth_stacks >= 1) or (combo_points == 4 and fth_stacks >= 2):
            return np.array([ACTIONS.index(PistolShot)])
        else:
            return np.array([ACTIONS.index(SinisterStrike)])

