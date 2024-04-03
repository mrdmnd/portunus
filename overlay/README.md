# Overlay
This is the core entry point to Portunus. The overlay will connect to the wow client's LUA addon data exporter and read the game state, then feed the game state into the inference engine for simulation and NN evaluation. 
Finally, the MCTS tree is visualized on screen with summary statistics.