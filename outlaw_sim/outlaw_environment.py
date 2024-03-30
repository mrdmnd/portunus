import gymnasium as gym
import numpy as np


# A helper type to keep track of our agent's state.
# Handle energy regen gracefully now.

class OutlawEnvironment(gym.Env):
    metadata = {"render_modes": ["console"], "render_fps": 30}
    EPISODE_TIMEOUT = 300.0

    def __init__(self, render_mode="console"):
        super(OutlawEnvironment, self).__init__()
        self.render_mode = render_mode
        self.agent_state = AgentState()
        self.action_space = gym.spaces.Discrete(len(ACTIONS))
        self.observation_space = self.agent_state.describe_observation_space()

    def reset(self, seed=None, options=None):
        super().reset(seed=seed, options=options)
        self.agent_state = AgentState()
        return (self.agent_state.get_observation(), self.agent_state.get_info())

    def step(self, action):
        reward = ACTIONS[action](self.agent_state)
        terminated = False
        truncated = (self.agent_state.sim_time >= OutlawEnvironment.EPISODE_TIMEOUT)
        observation = self.agent_state.get_observation()
        info = self.agent_state.get_info()
        return (observation, reward, terminated, truncated, info)
        

    def render(self):
        if self.render_mode == "console":
            print(self.agent_state)

    def close(self):
        pass


# SIM STUFF
def __repr__(self):
    return self.get_observation().__repr__()

def describe_observation_space(self):
    lower_bounds = np.array([cs.lower_bound for cs in self.state_scalars])
    upper_bounds = np.array([cs.upper_bound for cs in self.state_scalars])
    return gym.spaces.Box(low=lower_bounds, high=upper_bounds, shape=(len(lower_bounds),), dtype=np.float32)

def get_observation(self):
    return np.array([cs.value for cs in self.state_scalars], dtype=np.uint8) 

# Idea: return legal actions here so that algorithms can use that info for invalid-action-masking?! 
def get_info(self):
    return {
        "sim_time": self.sim_time,
        "legal_actions": [] # TODO IMPL ME
    }