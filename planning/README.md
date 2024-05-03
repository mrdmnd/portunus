# Planning
This module implements a thread-safe Monte Carlo Tree Search library (/search) as well as a game simulation library.

For simulation:
- Primitive: given a game state, be able to fast forward "one action":
 - Return the immediate reward from the action, the accrued reward from advancing (dots, auto attacks, etc), and the new state. 
 - Can be stochastic (account for crits, etc).

- To simulate a whole combat sequence, we take a game state, run a policy to select an action, then apply that action and use our primitive to advance the game forward.