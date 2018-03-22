# Design Document

This document describes (at a high level) the architecture and flow of the core
simulation loop, from start to end. We also discuss each of the important
primitives.

## Initialization
Our input is, roughly speaking, an encounter configuration, a player
configuration, and a policy. The encounter configuration contains information
pertaining to raid boss spawn timing, duration, and other raid events like
"extra damage taken" phases or "movement" phases, etc. The player configuration
contains a full set of player gear (notably, *unparsed* - we have to do the
parsing in the engine to get the CombatStats that the gearset sums up to) as
well as a set of talents, and a player specialization. 

First, during initialization, we set up the Player object object with the
appropriate CombatStats from their gear, and populate their Cooldown vector with
information from their spec + talents + gear (spec determines some base
cooldowns, talents might give a few more, and items on gear can give a few
more.)

Next, we handle raid event initialization:
We initialize the simulation engine with an "EventManager" object, which is
simply a container for function callbacks. Everything in our simulation is
designed to operate on a SimulationState pointer - all mutations are done
directly through this pointer. Raid events (like enemy spawns) are scheduled
into the EventManager by setting up a callback that mutates the SimulationState
vector of Enemies and inserting or removing the appropriate enemy. Events like
player damage multiplier phases are done similarly: at the appropriate time, we 
modify `state->player->stats->damage_modifier`.


