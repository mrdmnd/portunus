# Portunus

[![Build Status](https://travis-ci.org/mrdmnd/portunus.svg?branch=master)](https://travis-ci.org/mrdmnd/portunus)

[Current WebUI](http://portunuspl.us)

Portunus is the Roman god of keys and gates - an appropriate name for a tool that attempts to automatically optimize high-end Mythic Keystone runs in World of Warcraft!

Given a group of players, a set of "control policies" for those players, and a dungeon route configuration, Portunus simulates running through the entire key and returns a "final time" for your group to shoot for.
We allow the user to specify either a standard, deterministic control policy a la a traditional SIMC APL, or a pre-trained control policy in the form of a tensorflow model.

## Background

Why does Portunus exist? Aren't existing tools good enough? This document intends to capture the rationale behind implementing Yet Another World Of Warcraft Simulation Tool, as well as the roadmap ahead.

### Overview

Optimal gameplay in the MMO World of Warcraft (WOW) has been studied fairly extensively for nearly two decades by a number of researchers, collectively known as "theorycrafters." Over the course of the game's history, and multiple expansion packs, this community has pieced together tribal knowledge and shared information on "how to play" each of the game's class and specialization combinations.

The game has traditionally provided three roles for players to choose. Highly armored "tanks" actively keep the attacking enemies focused on them, "healers" ensure that the group remains alive until end of combat, and "direct damagers" - players whose job it is to deal damage as quickly as possible to the enemies - fling spells and swing swords at the baddies. In this last role, the traditional metric by which we distinguish good players from bad players is "DPS", or "damage per second". The implicit understanding here is that a player who does more DPS than another player, all else equal, is a "better" player. Obviously, in real-world experience, DPS is not the only important metric to evaluate players; often there are other useful things being done such as crowd control, dispels, positioning, etc - but barring other signals, DPS is the standard metric on which players are evaluated.

Like in many other games, there are sub-role classifications within these three roles (crowd controller, disabler, hybrid heal/damage roles, etc) but these three (heal, tank, DPS) represent the MMO "holy trinity", and are what most end-game player-vs-environment group content is balanced around.

Although optimization for healing and tanking is certainly an interesting problem, the performance of players in those roles is much harder to evaluate than the performance of a damage-dealing class. Traditional tools have largely avoided exploring this space, due to the complexity of the control policies required and the extra simulation fidelity necessary to capture the nuances of the non-damaging roles. Additionally, it is uniquely challenging to simulate healing, as oftentimes with perfect play (as any simulation will generally assume), DPS players are able to take close to zero effective damage during an encounter - the healer's role is one of "fixing mistakes" rather than being actively necessary to complete an encounter succesfully, so there is an implicit problem with simulating their impact when most damage is avoidable.

Additionally, it does not always make sense to optimize for HPS, as healing is largely a zero-sum game in high level group play: there is only so much damage going out to a group, and a finite set of healers to handle it. Healers optimizing for pure throughput will find themselves running out of resources towards the end of a fight; where healing is generally most important. Additionally, not all damage is equally important to heal quickly: the relative danger posed by a 10 damage spike when a player is at 11 health is much higher than the same 10 damage spike when a player is at 100 health.

Similarly, tanking is not so easily quantifiable: a good tank will simultaneously maximize their chances of staying alive while optimizing their damage, which poses a tradeoff in optimization space: a multi-objective problem with sometimes directly conflicting optimization goals. Tanks end up making a gearing choice between "offensive" and "defensive" profiles, or select an offensive or defensive action that consumes the same resource ("revenge" versus "ignore pain", for example). Historically, theorycrafters have developed policies and configurations for playing tanks "defensively" as well as "offensively", but evaluating the goodness of such policies has historically been out of scope for nearly all theorycrafting projects. In the mid 2010s, there was an attempt to unify these metrics with the invention of the Theck Meloree index, but this proved opaque and not easily understood by end users, and has largely been left by the wayside by the simulationcraft developer community.

Given the complexities mentioned above with simulation and optimization for healers and tanks, we explicitly choose to NOT focus on these roles. Instead, we concern ourselves in this work with the (already) herculean task of optimization across gameplay choices for "damage dealing" roles.

In fact, the entirety of the Portunus project can be boiled down to the following problem: for a given (predetermined) fight scenario with scripted enemy behavior, what choices can a damage dealer make to do as much damage as possible over the course of that fight? Equivalently, what choices can a damage dealer make to end a fight as quickly as possible, given access to a fixed pool of static gear options?

### Existing Work

There already exist tools that have attempted to answer many of these questions, but each of these tools has drawbacks. There is a rich history of spreadsheets and models, as well as rudimentary markov chain simulation for various classes, but by far the most successful project in the history of World of Warcraft has been SIMC, or "SimulationCraft". To this author's knowledge, SIMC was the first project that looked at damage optimization as a black-box simulation problem, and it was this new approach to the theorycrafting problem that propelled it past all of the other determinstic tools. There have been other attempts to model the game as a simulation (Ask Mr Robot, for one) but these other attempts have generally been poorly received by gamers due to their lack of transparency and community buy-in.

One fundamental issue that all of the existing tools have is their focus on optimization for a SINGLE PLAYER - you. What Portunus attempts to tackle is a slightly different problem: rather than optimization for individual players, we seek to optimize GROUP behavior: what choices and policies can a group select to maximize their probability of finishing a Mythic Keystone speedrun as quickly as possible. Here, the success metric is not "maximize DPS", but rather "minimize Total Time".

We intend to provide a formulation for specifying a "dungeon plan" - a collection of encounters (either boss encounters, or trash encounters) along with their setup time, and travel time between them. This plan will allow us to simulate a FULL GROUP as they progress through the dungeon and defeat enemies. 

### Formalization

DPS Optimization is a complicated instance of a classic planning problem, with an interesting twist. At a very abstract high level, there are effectively two planning components under the control of a player. These two components are "pre-combat player configuration" and "during-combat gameplay action policy".

The interesting twist in this problem is choices made during the planning phase tend to affect optimal choices in the action selection phase. For example, some policies favor gear-configurations with different stat balances or talent choices (maybe you have a spell that gets better and better with more critical strike rating on your gear, so you would want to cast that spell more frequently when you have a lot of crit rating, but almost never if your crit rating is low), and some stat balances and talent choices favor different action policies.

A very specific example of this conundrum (offline planning decisions influence online action selection) is the Assassination Rogue: at the beginning of the Battle for Azeroth expansion, there were largely considered to be two "viable" but distinct approachs to playing the assassination rogue - the "poison" build and the "bleed" build. The poison build required a large amount of mastery, because mastery (at the time) increased poison damage. If the rogue had a random gear configuration, it's unlikely that they would have enough mastery for the "poison" build to be good, but if they _specifically_ planned on using the poison build, they could set up a gearset with enough mastery to make it better than the "bleed" build.

Another piece of complexity is challenging to overcome: both of these components (offline- and online-planning) are thoroughly influenced by parameters of the encounter being simulated. The theorycrafting community has traditionally constructed different static player configurations and action policies for encounters that feature multiple persistent enemies ("AOE", or "area of effect", a term meaning an emphasis on abilities which do damage to multiple targets at once) and encounters that feature a single, high-health enemy ("Patchwork", named after a classical, canonical example enemy from the Naxxramas raid). However, this feature in SIMC has largely been underutilized (few players construct their own encounters), and many players make decisions solely on the results of a static, one-target encounter that is widely criticized as being non-representative of true, in-game combat scenarios.

Any new approach to automatically solve the character planning + policy discovery problems, therefore, must take ALL THREE parameters into account: pre-fight player configuration, during-fight player control policy, and encounter script details. The formalization we need to do optimization in a high-dimensional space is direct: our goal is to minimize the expected value of the objective function `DungeonTimeTaken(online action policy, offline configuration; encounter parameters)`. Note that in practice we tend to parameterize this function by encounter parameters: we'll fix the encounter parameters ahead of time ("stick to the plan, chums!" - Leeroy Jenkins), and attempt to learn both a good player configuration as well as a good policy to match that configuration for our particular encounter.

### Static Player Configuration

First, players need to make static choices about their character before combat ever begins: these pre-combat choices largely consists of two components. The first sub-component can be thought of as "equipment setup" and the second sub-component can be thought of as "talent choice" or specialization.

#### Equipment

Players have a number of empty "inventory slots" on their character when it is first created - these are slots like "helmet", "shoulders", "rings", "trinkets", "weapons", etc. A complete "gear configuration" is a mapping from each slot to a corresponding item that is equippable in that slot. Each damage specialization is constrained to using a subset of gear: a fury warrior generally does not wear cloth armor, and a shadow priest generally does not use a shield. Many items have synergistic effects with other items (i.e. item "tier sets" may impact gameplay in major ways: traditionally, the role of "raid tier set bonuses" has been to provide a gameplay-changing bonus to a specific player ability).

Additionally, some items (usually trinkets or weapons) have a random chance during combat to produce a magical effect (this is called a "proc", short for "programmed random occurrence"). Some items have an "on-use" ability that provides a powerful effect whose timing is controllable by a player.

Some items (legendary items) impact the behavior of player abilities entirely, and can provide a static on-equip effect similar in strength to player talents (see later section for more).

Most pieces of gear provide some amount of "primary stat" (intellect, agility, or strength) and some amount of "secondary stats" (versatility, mastery, critical strike, or haste). Each damage dealing specialization has a different interaction with these stats, but in combination, they form the core of player power progression: your character generally gets stronger only when you get better items.

Finally, players can use non-equippable consumable items, like food, augmenting runes, flasks, scrolls, and numerous other consumable items to get combat buffs.

#### Talents

Players have a choices of "talents" which further customize their specialization. In the current implementation, characters can select one talent from several rows of three choices each. This form of customization provides _OUTSIZED_ amounts of control on both optimal gearing as well as optimal policy selection. Some active talents grant new abilities to the player, while others passively modify the behavior of existing abilities completely. Some talents are better suited to multiple target combat, and other talents are optimized for single target damage. Players can only choose their talents once, before combat starts. In practice, some talents do not effect combat or damage meaningfully, and their impact can be ignored - these are generally called "utility" talents.

### Encounter Configuration

An encounter is essentially a script that specifies which targets are active at any given time, as well as when the player is moving or unable to attack said targets. We draw a lot of inspiration from the AskMrRobot fight encounter scripting system, which deserves praise for modelling the domain very cleanly.

### Action Policy

The core goal of the Portunus project is the automatic generation of good control policies through deep reinforcement learning. A common complaint with the old SIMC APLs ("action priority list") is that they were rarely optimized for mixed single-target damage and AOE damage, with profile maintainers sometimes electing to support both modes, but sometimes only focusing on single-target. 

We intend to support two types of policies, both of which are more flexible than the standard SIMC APL file. Both types of policies have *direct* access to the observable state from the simulation engine.

1) Explictly specified policies written using native Rust code that has direct access to the simulation state observable to a player, rather than a hard-to-understand textual configuration language like the SIMC APLs.

2) Implicitly specified policies given via a neural network + MCTS/AlphaZero-style architecture: these implicit policies are essentially defined by their neural network weights and the tree search parameters. 

An `action policy` represents a mapping from game-state to action taken - in some sense, it's "how you play". For each pair of (gearing configuration, encounter parameters) specified in the input, we seek to identify an optimal mapping of `state` to `action`. We formulate this problem as a classic "Markov Decision Problem": from each state `S`, there are a number of available actions `a`: for example, you might wait 1ms, or use ability X, use ability Y, etc. Each of these actions `a` has some transition probability of putting you into each of several potentially new states `S'`. You have some probability of receiving reward `R` from taking action `a` in state `S` and ending up in state `S'`: this is the damage value of the action at the time it was taken. 

The goal of framing this problem as a Markov Decision Problem is to allow us to use existing tools and solutions from the deep reinforcement learning community. Specifically, we're seeking to formally identify the action for each state that maximizes the gamma-discounted reward in the limit as time goes to infinity. This model is a good match for our problem domain.

The state-of-the-art technique for model-based reinforcement learning is AlphaZero - please see other resources for explanations.

### Similarity To Existing Tools In The Ecosystem / Prior Work

Several great tools have come about in the past several years to support World of Warcraft theorycrafters. Our goal is not necessarily to supplant these tools, but to build on their design success and incorporate their best parts. 

#### Debugging and Visualization w/ Warcraft Logs
The most notable improvement to the WOW theorycrafting world in the last few years has been the development of WarcraftLogs, by Kihra - this is a website where players can upload combat logs from various encounters, and receive information about their performance in a number of very well designed visuals.

I intend for Portunus's core simulation *debug output* to be styled much like a "combat log", formatted exactly like the WOW combat log is formatted, which can be uploaded *DIRECTLY* to Warcraft Logs (or a tool much like it!) for visualization. Years of working on SIMC has proven that a high level summary of a single execution pass can be useful for debugging strange behavior, and choosing a unified standard debug output will prove highly useful.

#### Gameplay Suggestion w/ HeroRotation

Additionally, Portunus' action policies should be exportable in the form of a rotation helper addon, like HeroRotation or Hekili or Ovale.
The idea here is to allow the addon to compute the "game state" by scraping information from the Wow LUA API, then use that state to run its control policy and suggest the next action to the player. We'll provide our own rotation helper that takes as input one of the trained control policies from a training run. 

#### Web Frontend for Simulation Configuration w/ Raidbots

Raidbots is a tool developed by Seriallos that provides an incredibly intuitive frontend to the SIMC core simulation tool. It allows users to select various "tasks" and then guides them through using a wizard to setup the appropriate SIMC input code. Portunus intends to provide a similar front end webserver + queueing service. Currently I own the domain portunuspl.us, and I'll host early versions here.

## Components

The goal is for Portunus to ultimately contain a few core components.

(1) A networked, distributed, deep-learning RPC service, for evaluating encounter configurations against a simulation engine (sort of like SIMC-lite). This component is intended to scale horizontally, and intended to be run behind a load balancer in a cloud computing environment.
Ideally we also setup some computation-pool sharing for compute resources; perhaps you can earn tokens by loaning your compute time to the distributed network, then you can spend them to prioritize your computation.

(2) A web UI front end for configuration (much like Method Dungeon Tools) that allow users to set up a dungeon encounter configuration that gets run by component (1) (group composition, gearing, monster pulls, CC, group skips, travel time, bloodlust + cooldown assignments, etc). Ideally, this configuration is exportable into MethodDungeonTools for on-the-fly visualization in-game of the intended route, as well as auto-generate notes indicating manual CD "mutex" assignments and likely time-to-completion timestamps for each trash pack.

This frontend should also provide a "debug" view for a single iteration of the simulator (much like warcraft logs) that provides timeline information, group cooldown usage, buff uptime, damage taken/done/etc.

(3) An addon for WOW much like HeroRotation that reads the world state and runs the appropriate policy model to determine the next action for a player to take.

## Installation

Portunus has several components. The core simulation service is built with Rust, and requires a full rust toolchain setup.
The web UI front end is built with python3 and flask. We assume a modern 64-bit architecture running some flavor of linux.

## Portunus Architecture Overview

### Simulation Service

We implement an RPC service that receives SimulationRequests and produces SimulationResponses. This is meant to be horizontally scalable. Each machine running this service is running a multithreaded simulation server binary. This is an embarassingly parallel problem, with minimal data dependencies. Simulation is mostly a CPU-bound problem, although it might be an interesting problem to attempt discrete event simulation on GPU.

To actually build the simulation, we need to get client data out of the game files. They're encoded in weird database formats, and we need this to be usable for our simulator. This is a non-interesting, solved problem by the SimC team, but we need this component, unless we want to hardcode every constant for every spell and every item. We rely on this data being available at compile time in the simulation engine code.

### Web Front End

Portunus is designed to be used from a web front end. We provide a web UI that allows users to set up their configuration (classes/specs/talents/gear) as well as their intended pull plan. 

The initial UI will allow users to specify pulls, when big CDs such as lust are used, when to use potions, and other encounter-specific things. We'll also provide a nice visualization a la warcraft logs of a single iteration of the simulator, showing what uptime people get on various buffs, what the damage breakdown looks like, etc.

### LUA Addon

This component takes the trained action policy for the given gearset and encounter configuration, and embeds itself directly into the player's in-game interface: it uses the existing WoW API to build up an equivalent representation of "player state" used by the action policy network, and then runs the model forward to determine the optimal action at any moment in time. In practice, we'll want to determine the optimal action at both _this instant_ as well as a few times in the future, to give the player a bit of predictive breathing room. This is similar (almost exactly identical) to existing addons Ovale or HeroRotation.
