# Design Document

"Bad programmers worry about the code. Good programmers worry about data
structures and their relationships."- Linus Torvalds

This document describes (at a high level) the architecture and flow of the core
simulation loop, all the way from start to end. Additionally, we discuss each of
the important simulation primitives.

See a few links for GPU Discrete Event Simulation:
https://studentnet.cs.manchester.ac.uk/resources/library/thesis_abstracts/BkgdReportsMSc11/Kajabadi-Farhad-bkgd-rept.pdf
https://smartech.gatech.edu/bitstream/handle/1853/53887/SWENSON-DISSERTATION-2015.pdf
http://yuhaozhu.com/pubs/todaes11.pdf

## Initialization

### Parse Configuration Proto

Our input is, roughly speaking, the union of an
encounter configuration, a player configuration, and a policy. The encounter
configuration contains information pertaining to raid boss spawn timing,
duration, and other raid events like "extra damage taken" phases or "movement"
phases, etc. The player configuration contains a full set of player gear
(notably, *unparsed* - we have to do the parsing in the engine to get the
CombatStats that the gearset sums up to) as well as a set of talents, and a
player specialization.  The first step (even before we initialize things) is to
parse all of the configurations into their engine-internal forms.

### Encounter Configuration Parse

For the encounter configuration, we grab the
minimum and maximum encounter time and sample a valid encounter time uniformly
at random between the two. We also create RaidEvents from each event listed.

### Player Configuration Parse

### Policy Parse
We need to turn the policy proto from the configuration proto into an internal
representation of a Policy class. This step depends on whether the policy we've
received from the input is specified as a priority list, or as a neural network.

### Player Initialization

First, during initialization, we set up the Player
object.  appropriate CombatStats from their gear, and populate their Cooldown
vector with information from their spec + talents + gear (spec determines some
base cooldowns, talents might give a few more, and items on gear can give a few
more.)

## Raid Event Scheduling Next up, we schedule raid events.

We initialize the simulation engine with an "EventManager" object, which is
simply a container for function callbacks. Everything in our simulation is
designed to operate on a SimulationState pointer - all mutations are done
directly through this pointer. Raid events (like enemy spawns) are scheduled
into the EventManager by setting up a callback that mutates the SimulationState
vector of Enemies and inserting or removing the appropriate enemy. Events like
player damage multiplier phases are done similarly: at the appropriate time, we
modify `state->player->stats->damage_modifier`.


## Core Loop

We split the core simulation loop into three phases:
1) Identify the time of the next tick with at least one event scheduled to run,
and run the clock forward until that time is reached.

2) Execute all events scheduled unconditionally on this tick. These events are
allowed to modify simulation state. If they do modify simulation state on this
tick, we may need to add some triggers to this tick's "trigger" list.

3) Resolve triggers. State changes that depend on the triggers from step 2) are
execute here. To resolve a trigger, we look through our registry of all things
that can be triggered by that trigger, and check for its execution. Executing
the callbacks for triggers may put new triggers on the stack, so while the
trigger stack is non-empty, we loop through step 3).

And that's it! We just loop through here until the simulation time is up.
