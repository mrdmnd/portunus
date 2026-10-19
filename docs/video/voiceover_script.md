# Portunus Burn RL — 3Blue1Brown Narrative Voiceover Script

This script is synthesized via TTS and muxed directly into `docs/video/portunus_rl_3b1b.mp4`.

## ACT I `[00:00 - 00:53]` — How Portunus Fits Together: Self-Play RL in Pure Rust + Burn

> How can an agent learn a complex World of Warcraft damage rotation completely from scratch, without reading a human guide? In Portunus, six Rust crates form a closed AlphaZero-style reinforcement learning loop, orchestrated by train R L in crates, run, source, train dot R S. First, Sim Env wraps a deterministic, cloneable discrete-event combat kernel. Second, Seat Obs Featurizer encodes the player's realistic observation into 132 floats, while Spec Action Space maps legal spells into 35 discrete actions. Third, Monte Carlo Tree Search forks the combat world 24 times per decision to discover high-damage combos. Fourth, Self Play Collector records those search trajectories. Fifth, Burn Trainer updates our two-headed Policy Value Network using Autodiff and Adam W. And finally, Burn Distiller compresses the trained network into a standalone Lua addon.

## ACT II `[00:53 - 01:45]` — From Combat State to Burn Tensors: SeatObs -> PolicyValueNet

> Let's zoom in on a single decision point for an Elemental Shaman using the Stormbringer hero tree. In crates, run, source, observe dot R S, Seat Obs Featurizer reads only what a real player can see: 65 Maelstrom, an open global cooldown, 4.2 seconds left on Flame Shock, two stacks of Stormkeeper, and Voltaic Blaze ready to cast. This 132-dimensional vector flows into Policy Value Net, defined in crates, policy, source, lib dot R S. Built on Burn 0.22's Flex backend, the network splits into two Multi-Layer Perceptron towers. The Policy Tower outputs logits across all 35 actions, where illegal or on-cooldown spells like Ascendance are masked to negative infinity before softmax. Meanwhile, the Value Tower predicts a single scalar: the expected discounted damage over the next six seconds.

## ACT III `[01:45 - 02:37]` — Lookahead Tree Search: PUCT MCTS Over Cloned Combat Worlds

> Why doesn't the network just act on its initial guesses? Because lookahead search acts as a policy amplifier. In M C T S search, at line 632 of crates, policy, source, lib dot R S, we clone the entire combat simulation 24 times over a six-second lookahead horizon. Because raw damage numbers vary wildly across specs, we track the minimum and maximum Q values across the tree to normalize every branch into the unit interval, zero to one, and we scale truncated rollouts by horizon over elapsed time so fast instants and slow casts are compared by true D P S rate. Look at the right branch: when M C T S tests Voltaic Blaze, it sees that it applies Flame Shock immediately, turning the next Lava Burst into a guaranteed 100 percent critical strike. Q-sharpening concentrates 76 percent of the target probability onto Voltaic Blaze.

## ACT IV `[02:37 - 03:42]` — Hillclimbing Single-Target DPS: Elemental Shaman [Stormbringer]

> Now watch what happens when we close the loop and hillclimb across six iterations. At Iteration 0, the untrained Burn network is completely clueless: it spams Frost Shock 419 times, dealing just 1,558 D P S, or 4.3 percent of the hand-written SimulationCraft priority list. In crates, learn, source, lib dot R S, Burn Trainer minimizes masked cross-entropy against the M C T S targets and mean squared error against the discounted returns. After just 107 decisions in Iteration 1, the network leaps to 39,944 D P S, beating the human script. By Iteration 6, policy loss drops from 1.25 to 0.60, and the pure neural network—running greedily at temperature zero with no tree search at test time—hits 45,797 D P S. That is 27.7 percent higher than the baseline script, because RL discovered that casting Voltaic Blaze on cooldown unlocks massive Flame Shock and Lava Burst synergy while pruning Frost Shock to zero.

## ACT V `[03:42 - 04:16]` — Knowledge Distillation & Zero-Dependency Export to model.lua

> Finally, in crates, export, source, lib dot R S, we prepare the trained policy for the game client. Burn Distiller trains a compact, single-tower Student Net with just 32 hidden units to imitate the teacher across held-out combat states, achieving 99.1 percent agreement on Stormbringer and 100 percent on Arms Warrior. Lua Addon Emitter then serializes the Burn module weights directly into Portunus Rotation, model dot lua: a 57-kilobyte, zero-dependency Lua table that executes a pure ReLU forward pass in micro-seconds.
