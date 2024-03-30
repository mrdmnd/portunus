import numpy as np
from outlaw_environment import OutlawEnvironment
from human_controller import HumanController
from stable_baselines3 import PPO
from stable_baselines3 import DQN
from stable_baselines3.common.callbacks import BaseCallback
from stable_baselines3.common.env_checker import check_env
from stable_baselines3.common.evaluation import evaluate_policy

env = OutlawEnvironment()
check_env(env, warn=True)

model = PPO.load("ppo_outlaw", env=env)
#model = PPO("MlpPolicy",
#    env, 
#    learning_rate=0.5,
#    verbose=1, 
#    tensorboard_log='./tensorboard_logdir/',
#)
model.learn(
    total_timesteps=int(2e5), 
    progress_bar=True,
)
model.save("ppo_outlaw")

#model = PPO.load("ppo_outlaw", env=env)

human_controller = HumanController()

print("Human controller run.")
vec_env = model.get_env()
obs = vec_env.reset()
cumulative_human_reward = 0
for _ in range(300):
    vec_env.render("console")
    action = human_controller.predict(obs, deterministic=True)
    print("human action: ", action[0])
    obs, rewards, dones, info = vec_env.step(action)
    cumulative_human_reward += rewards[0]

print("AI controller run.")
vec_env = model.get_env()
obs = vec_env.reset()
cumulative_ai_reward = 0
for _ in range(300):
    vec_env.render("console")
    action, _states = model.predict(obs, deterministic=True)
    print("ai action: ", action[0])
    obs, rewards, dones, info = vec_env.step(action)
    cumulative_ai_reward += rewards[0]

print("Human reward: ", cumulative_human_reward / 300.0)
print("AI reward: ", cumulative_ai_reward / 300.0)

# poetry run python3 outlaw_agent.py