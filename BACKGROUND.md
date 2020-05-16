# Background and Motivation

Why does Portunus exist? Aren't existing tools good enough? This document intends to capture the rationale behind implementing Yet Another World Of Warcraft Simulation Tool.

## Overview

Optimal gameplay in the MMO World of Warcraft (WOW) has been studied fairly extensively for nearly two decades by a number of researchers, collectively known as "theorycrafters." Over the course of the game's history, and multiple expansion packs, this community has pieced together tribal knowledge and shared information on "how to play" each of the game's class and specialization combinations.

The game has traditionally provided three roles for players to choose. Highly armored "tanks" actively keep the attacking enemies focused on them, "healers" ensure that the group remains alive until end of combat, and "direct damagers" - players whose job it is to deal damage as quickly as possible to the enemies - fling spells and swing swords at the baddies. In this last role, the traditional metric by which we distinguish good players from bad players is "DPS", or "damage per second". The implicit understanding here is that a player who does more DPS than another player, all else equal, is a "better" player.

Like in many other games, there are sub-role classifications within these three roles (crowd controller, disabler, hybrid heal/damage roles, etc) but these three (heal, tank, DPS) represent the MMO "holy trinity", and are what most end-game group content is balanced around.

Although optimization for healing and tanking is certainly an interesting problem, the performance of players in those roles is much harder to evaluate than the performance of a damage-dealing class. Traditional tools have largely avoided exploring this space, due to the complexity of the control policies required and the extra simulation fidelity necessary to capture the nuances of the non-damaging roles. Additionally, it is uniquely challenging to simulate healing, as oftentimes with perfect play (as any simulation will generally assume), DPS players are able to take close to zero effective damage during an encounter - the healer's role is one of "fixing mistakes" rather than being actively necessary to complete an encounter succesfully, so there is an implicit problem with simulating their impact when most damage is avoidable.

Additionally, it does not always make sense to optimize for HPS, as healing is largely a zero-sum game in high level group play: there is only so much damage going out to a group, and a finite set of healers to handle it. Healers optimizing for pure throughput will find themselves running out of resources towards the end of a fight; where healing is generally most important. Additionally, not all damage is equally important to heal quickly: the relative danger posed by a 10 damage spike when a player is at 11 health is much higher than the same 10 damage spike when a player is at 100 health.

Similarly, tanking is not so easily quantifiable: a good tank will simultaneously maximize their chances of staying alive while optimizing their damage, which poses a tradeoff in optimization space: a multi-objective problem with sometimes directly conflicting optimization goals. Tanks end up making a gearing choice between "offensive" and "defensive" profiles, or select an offensive or defensive action that consumes the same resource ("revenge" versus "ignore pain", for example). Historically, theorycrafters have developed policies and configurations for playing tanks "defensively" as well as "offensively", but evaluating the goodness of such policies has historically been out of scope for nearly all theorycrafting projects. In the mid 2010s, there was an attempt to unify these metrics with the invention of the Theck Meloree index, but this proved opaque and not easily understood by end users.

Given the complexities mentioned above with simulation and optimization for healers and tanks, we explicitly choose to NOT focus on these roles. Instead, we concern ourselves in this work with the (already) herculean task of optimization across gameplay choices for "damage dealing" roles.

In fact, the entirety of the Portunus project can be boiled down to the following problem: for a given (predetermined) fight scenario with scripted enemy behavior, what choices can a damage dealer make to do as much damage as possible over the course of that fight? Equivalently, what choices can a damage dealer make to end a fight as quickly as possible, given access to a fixed pool of static gear options?

## Existing Work

There already exist tools that have attempted to answer many of these questions, but each of these tools has drawbacks. There is a rich history of spreadsheets and models, as well as rudimentary markov chain simulation for various classes, but by far the most successful project in the history of World of Warcraft has been SIMC, or "SimulationCraft". To this author's knowledge, SIMC was the first project that looked at damage optimization as a black-box simulation problem, and it was this new approach to the theorycrafting problem that propelled it past all of the other determinstic tools. There have been other attempts to model the game as a simulation (Ask Mr Robot, for one) but these other attempts have generally been poorly received by gamers due to their lack of transparency and community buy-in.

One fundamental issue that all of the existing tools have is their focus on optimization for a SINGLE PLAYER - you. What Portunus attempts to tackle is a slightly different problem: rather than optimization for individual players, we seek to optimize GROUP behavior: what choices and policies can a group select to maximize their probability of finishing a Mythic Keystone speedrun as quickly as possible. Here, the success metric is not "maximize DPS", but rather "minimize Total Time".

We intend to provide a formulation for specifying a "dungeon plan" - a collection of encounters (either boss encounters, or trash encounters) along with their setup time, and travel time between them. This plan will allow us to simulate a FULL GROUP as they progress through the dungeon and defeat enemies. 

## Formalization

DPS Optimization is a complicated instance of a classic planning problem, with an interesting twist. At a very abstract high level, there are effectively two planning components under the control of a player. These two components are "pre-combat player configuration" and "during-combat gameplay action policy".

The interesting twist in this problem is choices made during the planning phase tend to affect optimal choices in the action selection phase. For example, some policies favor gear-configurations with different stat balances or talent choices (maybe you have a spell that gets better and better with more critical strike rating on your gear, so you would want to cast that spell more frequently when you have a lot of crit rating, but almost never if your crit rating is low), and some stat balances and talent choices favor different action policies.

A very specific example of this conundrum (offline planning decisions influence online action selection) is the Assassination Rogue: at the beginning of the Battle for Azeroth expansion, there were largely considered to be two "viable" but distinct approachs to playing the assassination rogue - the "poison" build and the "bleed" build. The poison build required a large amount of mastery, because mastery (at the time) increased poison damage. If the rogue had a random gear configuration, it's unlikely that they would have enough mastery for the "poison" build to be good, but if they _specifically_ planned on using the poison build, they could set up a gearset with enough mastery to make it better than the "bleed" build.

Another piece of complexity is challenging to overcome: both of these components (offline- and online-planning) are thoroughly influenced by parameters of the encounter being simulated. The theorycrafting community has traditionally constructed different static player configurations and action policies for encounters that feature multiple persistent enemies ("AOE", or "area of effect", a term meaning an emphasis on abilities which do damage to multiple targets at once) and encounters that feature a single, high-health enemy ("Patchwork", named after a classical, canonical example enemy from the Naxxramas raid). However, this feature in SIMC has largely been underutilized (few players construct their own encounters), and many players make decisions solely on the results of a static, one-target encounter that is widely criticized as being non-representative of true, in-game combat scenarios.

Any new approach to automatically solve the character planning + policy discovery problems, therefore, must take ALL THREE parameters into account: pre-fight player configuration, during-fight player control policy, and encounter script details. The formalization we need to do optimization in a high-dimensional space is direct: our goal is to minimize the expected value of the objective function `DungeonTimeTaken(online action policy, offline configuration; encounter parameters)`. Note that in practice we tend to parameterize this function by encounter parameters: we'll fix the encounter parameters ahead of time ("stick to the plan, chums!" - Leeroy Jenkins), and attempt to learn both a good player configuration as well as a good policy to match that configuration for our particular encounter.

## Player Configuration

First up, players need to make choices about their character before combat ever begins: these pre-combat choices largely consists of two components. The first sub-component can be thought of as "gearing/item usage" and the second sub-component can be thought of as "talent choice" or specialization.

### Gearing and Itemization

Players have a number of empty "inventory slots" on their character when it is first created - these are slots like "helmet", "shoulders", "rings", "trinkets", "weapons", etc. A complete "gear configuration" is a mapping from each slot to a corresponding item that is equippable in that slot. Each damage specialization is constrained to using a subset of gear: a fury warrior generally does not wear cloth armor, and a shadow priest generally does not use a shield. Many items have synergistic effects with other items (i.e. item "tier sets" may impact gameplay in major ways: traditionally, the role of "raid tier set bonuses" has been to provide a gameplay-changing bonus to a specific player ability).

Additionally, some items (usually trinkets or weapons) have a random chance during combat to produce a magical effect. Some items have an "on-use" ability that provides a powerful effect whose timing is controllable by a player.

Some items (legendary items) impact the behavior of player abilities entirely, and can provide a static on-equip effect similar in strength to player talents (see later section for more).

Most pieces of gear provide some amount of "primary stat" (intellect, agility, or strength) and some amount of "secondary stats" (versatility, mastery, critical strike, or haste). Each damage dealing specialization has a different interaction with these stats, but in combination, they form the core of player power progression: your character gets stronger only when you get better items.

Finally, players can use non-equippable consumable items, like food, augmenting runes, flasks, scrolls, and numerous other consumable items to get combat buffs.

Potions are a particularly impactful consumable that, due to a quirk of history, can be chugged once right before combat begins, and then again during combat. For clarity and historical congruence, we separate the use of a combat-potion into the "online action policy" component, as the timing of this use is an important player choice! The rest of the consumables (flask, runes, food, pre-pot, etc) can safely be baked into the static pre-combat "player configuration" component.

### Talents

Players have a choices of "talents" which further customize their specialization. In the current implementation, characters can select one talent from several rows of three choices each. This form of customization provides _OUTSIZED_ amounts of control on both optimal gearing as well as optimal policy selection. Some talents grant new abilities to the player, while others modify the behavior of existing abilities completely. Some talents are better suited to multiple target combat, and other talents are optimized for single target damage. Players can only choose their talents before combat starts. In practice, some talents do not effect combat or damage meaningfully, and their impact can be ignored. This will help prune the search space a bit.

## Encounter Configuration

An encounter is essentially a script that specifies which targets are active at any given time, as well as when the player is moving or unable to attack said targets. We draw a lot of inspiration from the AskMrRobot fight encounter scripting system, which deserves praise for modelling the domain very cleanly.

TODO(mrdmnd) - add more details about how we intend to model the fights of a Mythic+ keystone run here.

## Action Policy

One particular goal of the Portunus project is the automatic generation of good control policys through deep reinforcement learning. A common complaint with the old SIMC APLs ("action priority list") is that they were rarely optimized for mixed-use single target damage and area of effect damage. We intended to support both explictly specified policies (written using native Rust directly, rather than a hard-to-understand textual configuration language like the SIMC APLs), as well as a mode that uses less-easily-interpretable black box RL models.

An `action policy` represents a mapping from game-state to action taken. For each pair of (gearing configuration, encounter parameters), we seek to identify an optimal mapping of `state` to `action`. We formulate this problem as a classic "Markov Decision Problem": from each state `S`, there are a number of available actions `a`: for example, you might wait 1ms, or use ability X, use ability Y, etc. Each of these actions `a` has a transition probability of putting us in each of several new states `S'`. We derive reward `R` from taking action `a` in state `S`: this is the damage value of the action at the time it was taken. Finally, we discount future rewards by an exponential decay parameterized by `gamma` in order to bias the learning towards immediate payback.

The goal of the Markov Decision Problem is to formally identify the action for each state that maximizes the gamma-discounted reward in the limit as time goes to infinity. This is a good match for our problem domain, and we choose to utilize the same formulation for identifying the optimal policy.

One technique in reinforcement learning is Q-learning: rather than learn directly "which action is optimal for this state?" and proceed recursively from there, an equivalent formulation is "what states are the best states to find yourself, in general?" and choose the actions that lead to those states with highest probability. Google has famously used this technique to great effect to learn how to play the game GO better than all top human experts.

## Interaction / Similarity To Existing Tools In The Ecosystem

Several great tools have come about in the past five years to support World of Warcraft theorycrafters. The most notable two are the development of WarcraftLogs, by Kihra - this is a website that you can upload your combat log from various encounters, and the site presents information about your performance in a number of very well designed visuals.

### Debugging and Visualization w/ Warcraft Logs

I intend for Portunus's core simulation debug output to be a "combat log" formatted exactly like the WOW combat log is formatted, which can be uploaded *DIRECTLY* to Warcraft Logs (or a tool much like it!) for visualization. Years of working on SIMC has proven that a high level summary of a single execution pass can be useful for debugging strange behavior.

### Gameplay Suggestion w/ HeroRotation

Additionally, Portunus' action policies should be exportable to various Rotation Helper addons, like HeroRotation. These addons compute the "game state" by scraping information from the WoW LUA API, and then use that state to suggest the next action to the player. This is a natural fit for portunus, as our action policies are modelled the same way - compute the game state, then suggest an action.

### Web Frontend for Simulation Configuration w/ Raidbots

Raidbots is a tool developed by Seriallos that provides an incredibly intuitive frontend to the SIMC core simulation tool. It allows users to select various "tasks" and then guides them through using a wizard to setup the appropriate SIMC input code. Portunus intends to provide a similar front end webserver + queueing service. Currently I own the domain portunuspl.us...
