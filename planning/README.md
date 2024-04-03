# Planning
This module implements a thread-safe Monte Carlo Tree Search library in Rust (/search) as well as a game simulation library in Rust.

For simulation:
- Primitive: given a game state, be able to fast forward "one action":
 - Return the immediate reward from the action, the accrued reward from advancing (dots, auto attacks, etc), and the new state. 
 - Can be stochastic (account for crits, etc).
