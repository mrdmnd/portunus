# policygen
Policygen is an automated action-priority-list (policy) generation tool for World of Warcraft DPS classes.

## High Level Overview

Optimal gameplay in the MMO World of Warcraft (WOW) has been studied
fairly extensively for more than a decade by a number of researchers,
collectively known as "theorycrafters." Over the course of the game's
history, and multiple expansion packs, this community has pieced
together tribal knowledge and shared information on "how to play" each
of the game's class and specialization combinations.

The game has traditionally provided a three-role distinction for players
to choose: "tanking," or actively keeping the attacking enemies focused
on a high-defense character; "healing", or ensuring that the group
remains alive until the end of combat, and "damaging/DPSing", or dealing
as much damage as possible to the enemy characters. Here, "DPS" stands
for "damage per second", which is the traditional optimization metric
for this role.

Like in many other games, there are sub-role classifications within
these three roles (crowd controller, disabler, hybrid heal/damage roles,
etc) but these three represent the MMO "holy trinity", and are what all
end-game group content is balanced around.

Although optimization for healing and tanking is certainly interesting,
those roles are markedly more nuanced in their evaluation: it does not
always make sense to optimize for "healing per second", as healing is
largely a zero-sum game in high level group play: there is only so much
damage going out to a group, and a cadre of healers to handle it.
Additionally, not all damage is created equal: the relative danger posed
by a 10 damage spike when a player is at 10 health is much higher than the
same spike when a player is at 100 health. Similarly, tanking is not so
easily quantifiable: a good tank will both maximize their chances of not
dieing while also optimizing their personal damage-dealing abilities, so
this poses a tradeoff in optimization space, a multi-objective problem
with frequently directly conflicting optimization goals. Historically,
theorycrafters have developed policies for playing tanks "defensively"
as well as "offensively", but evaluating the "goodness" of such policies
is out of scope for this project.

Given the complexities mentioned above with simulation and optimization
for healers and tanks, we explicitly choose to NOT focus on these roles.
Instead, we concern ourselves in this work with the (already) herculean
task of optimization across the gameplay choices for generalized "damage
dealing" roles.

We consider the following problem: for a given predetermined fight
"script" specifying enemy actor positions/behaviors/health/ability usage,
what choices can I make as a player to do as much damage as possible over
the course of that fight?

## Formalization
DPS Optimization is a (very) complicated instance of a "planning" problem,
with an interesting twist. At a very abstract high level, there are
effectively two planning components under the control of a player. These
two components are "pre-combat player configuration" and "gameplay action policy".

The interesting twist in this problem is that optimal choice for each of
these components affects the optimal choice for the other: some action
policies favor configurations with different stat balances or talent choices,
and some stat balances and talent choices favor different action policies.

Both of these planning components are also impacted by parameters of the
encounter being simulated: for example, classically the theorycraft
community has drawn up different player configurations and action
policies for encounters that feature multiple persistent enemies ("AOE",
or "area of effect", a term meaning an emphasis on abilities which
do damage to multiple targets at once) and encounters that feature a
single, high-health enemy ("Patchwork", named after a classical,
canonical example enemy from the Naxxramas raid).

The formalization we need to do optimization in a high-dimensional space
is direct: our goal is to maximize the expected value of the function
`SIMDPS(player configuration, action policy, encounter parameters)`.

### Player Configuration
First, players need to make choices about their character before combat
ever begins: these pre-combat choices largely consists of two components.
The first sub-component can be thought of as "gearing/item usage" and the second
sub-component can be thought of as "talent choice".

#### Gearing and Itemization
Players have a number of empty "inventory slots" on their character when
it is first created - these are slots like "helmet", "shoulders",
"rings", "trinkets", "weapons", etc. A complete "gear configuration" is
a mapping from each slot to a corresponding item that is equippable in
that slot. Different damage specializations have different armor
affinities, which largely serve to limit the search space. For example,
a fury warrior generally does not wear cloth armor, and a shadow priest
generally does not use a shield. Many items have synergistic effects
with other items (i.e. item "sets" may impact gameplay in limited- to
major ways: traditionally, the role of "raid tier set bonuses" has been
to provide a gameplay-changing bonus). Additionally, some items (usually
trinkets or weapons) have a random chance during combat to generate a
magical effect. Some items have an "on-use" ability that provides a
controllable, power effect at a time deemed by a player to be useful.
Some items (legendary items) impact the behavior of player abilities
entirely, and provide a static on-equip effect similar in strength to
player talents (see later section for more).

Most pieces of gear provide some amount of "primary stat" (intellect,
agility, or strength) and some amount of "secondary stats" (versatility,
mastery, critical strike, or haste). Each damage dealing specialization
has a different interaction with these stats, but in combination, they
form the core of player power progression: your character "gets
stronger" exactly when you have better items.

Finally, there are non-equippable items in the form of consumables.
Players can use food, augment runes, flask, scrolls, and numerous other
consumable items to get a stat bonus or buff for a specified duration before
combat stats, and during combat, they can use health potions or damage
potions. For clarity and historical congruence, we separate pre- and
during-combat potion use into the "policy" component but the rest of the
consumables (flask, runes, food, etc) into the pre-combat "player
configuration" component.

#### Talents
Players have a choices of "talents" which further customize their
specialization. In the current implementation, characters can select one
talent from several rows of three choices each. This form of customization
provides *OUTSIZED* amounts of control on both optimal gearing as well
as optimal policy selection. Some talents grant new abilities to the
player, while others modify the behavior of existing abilities
completely. Some talents are better suited to multiple target combat,
and other talents are optimized for single target damage. Players can
choose their talents before combat starts. In practice, some talents do
not effect combat or damage meaningfully, and their impact can be
ignored.

### Action Policy
The core of this project involves reinforcement-learning on this component.

An `action policy` represents a mapping from game-state to action taken.
For each pair (gearing configuration, encounter parameters), we seek to
identify an optimal mapping of `state` to `action`. We formulate this problem
as a classic "Markov Decision Problem": from each state `S`, there are a
number of available actions `a`: for example, you might wait 1ms, or use
ability X, use ability Y, etc. Each of these actions `a` has a transition
probability of putting us in each of several new states `S'`. We derive
reward `R` from taking action `a` in state `S`: this is the damage value of the action at the
time it was taken. Finally, we discount future rewards by an exponential
decay parameterized by `gamma` in order to bias the learning towards
immediate payback.

The goal of the Markov Decision Problem is to formally identify the
action for each state that maximizes the gamma-discounted reward in the limit
as time goes to infinity. This is a good match for our problem domain,
and we choose to utilize the same formulation for identifying the
optimal policy.

One technique in reinforcement learning is Q-learning: rather than learn
directly "which action is optimal for this state?" and proceeding
recursively from there, an equivalent formulation is "what states are
the best states to find yourself, in general?" and choose the actions
that lead to those states. Google has famously used this technique to
great effect to learn how to play the game GO as well as top human
experts.

## Components
  - GPU-accelerated simulation engine
  - Class Modules
  - Spec Modules
  - Deep Q-Learning policy network
  - Output for given (player configuration, fight parameters) is a
    policy, but not necessarily human-interpretable. Build a LUA addon
    that imports this policy in-game (much like easy-raid) so that players
    can enact it.
  - Can we build a tool that translates policy output into SIMC APL, AMR
    APL for verification?


