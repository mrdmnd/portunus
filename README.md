# Portunus
Portunus is the Roman god of keys and gates - an appropriate name for a suite of tools that attempts to automatically optimize a character control policy for performing keystone speedruns in World of Warcraft. We provide an end-to-end implementation of DeepMind's AlphaZero Deep Reinforcement Learning algorithm to automatically reveal the correct actions to take in any given situation.

## Historical Context and Problem Motivation
See (this document)[HISTORY.md] for historical context on the problem space.

## Project Goals
Core goal: build a framework for bringing AlphaZero style MCTS and policy improvement to World of Warcraft PVE.

## Installation
- (1) Load the data-export addon into your client.
- (2) Launch the WoW Client.
- (3) Compile and launch the overlay executable, which will launch the simulation service and the tree search service.

If you want to run your own training or fine tune, plase look at the scripts in /training.

## Components and Design
### Overlay (AlphAzeroth) (Rust)
We'll have a debug overlay (written in rust) that presents a nice rotation-helper style icon, as well as details on the MCTS tree and the likely next action. We'll try to have this update every 0.250ms, depending on the inference budget (may be slower, though anything slower than about 500ms is not useful).

This overlay will show a little sideways view of the MTCS tree (configurable depth visibility), with the selected action on the top branch of the tree. We'll have do do a little figma/design work here.

The cool thing about this is that if you don't want to press a cooldown (ostensibly, the top selection on the tree) then the next-best "hold CDs" action will be the action just below it!

For debug/testing purposes, we'll allow the user to specify either 
- (a) a standard, deterministic, human-programmed control policy (a la a traditional SIMC APL), or
- (b) use a model-free RL policy (just the AlphaZero policy net), or
- (c) use a model-based RL policy (Alphazero) with MCTS search/lookahead based on simulation.

(implementors note) See this overlay: https://github.com/Urcra/outside-auras/tree/master/src

### In-game Addon  (LUA)
WoW addons are generally programmatically forbidden from communicating game information through side channels to other processes. 

Because the set of things we're trying to do is more computationally expensive (GPU- and MTCS- resource intensive, at inference time) than what we can feasibly do inside of a sandboxed WoW LUA addon,
we need to exfiltrate the relevant game state information from the client to the overlay. What we need for inference is a complete representation of "game state" - the player's cooldowns, as well as targets in range, their health status, etc.

Much of this information is available legally through the LUA API - see HeroRotation for a great example. We'll repackage a lot of this state-extraction code in an in-game addon, but we'll export an encoded representation of the game state via a "pixel barcode" that our overlay program AlphAzeroth can scan and pass to the inference engine.

NOTE: this is probably against the TOS but we're only exporting data that would be otherwise available through the (legal) API - no unlocker necessary. If there's some way to write binary data over an RPC channel through the game in a legitimate manner, that'd be usable too.
This is NOT a bot and does not automate any of the decision making to a script - you still have to press the suggested button and move your character! There's no memory writing happening here.

I believe this is a slightly thorny position to take - this is a research project, not intended to gain an unfair advantage over other players. Ideally I could write everything that the overlay does IN GAME in LUA, but we'd either have to (a) get access to GPU resources for the NN evaluations or (b) rewrite the whole thing to use CPU only, as well as a pytorch-style library directly in LUA. This is not impossible, but as a proof of concept I'm going with the "slightly sketchier" approach at first.

### Planning Module (Rust)
The overlay (main program) will pass information from the in-game addon to an MCTS-style AlphaZero planner module. This is two components: simulation, and search.

This planning server component will start up a single time when the overlay launches, configure itself with the current player information, and then block until it receives a request for simulation.

This component will run locally on the user's machine, and be multithreaded. We'll tune the parallelism such that we can hit the correct number of rollouts given our computation budget.

We'll ALSO use the simulation component of this planning module during training -- we need to be able to predict the next state from the current one. In theory, this server should be horizontally scalable!

### Training (Python)
This component is responsible for training the (policy, value) network for each supported class. We'll use the same ideas from the AlphaZero paper, along with our simulation engine from above. Training will be done with RAY, and model checkpoints will be saved to the /models directory.

The model will be exported as a TorchScript file (see [here](https://pytorch.org/docs/stable/jit.html)) for use in the overlay program.

We'll try to set this up so it can be a distributed training run, depending on the nature of how slow/expensive it is to train.

Models will be available for free.

### Finetuning (Python)
This component is responsible for performing transfer learning from the base trained network to SPECIALIZE a network based on your character exactly. If you have more crit/haste/vers/mastery or different items that might affect the training, or a different talent setup -- go here to fine-tune the model for your specific character. It's possible that I may move this to a website, where you can upload your character and queue a model fine-tune run.

This is a "nice-to-have" and will likely be the last thing implemented.