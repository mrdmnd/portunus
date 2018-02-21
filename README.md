# PolicyGen

PolicyGen is a tool for discovering optimal character control policies in World
of Warcraft. We compute an optimal policy through the use of high-fidelity
simulation tools and deep reinforcement learning.

## High Level Overview

Optimal gameplay in the MMO World of Warcraft (WOW) has been studied fairly
extensively for more than a decade by a number of researchers, collectively
known as "theorycrafters." Over the course of the game's history, and multiple
expansion packs, this community has pieced together tribal knowledge and shared
information on "how to play" each of the game's class and specialization
combinations.

The game has traditionally provided three roles for players to choose. The
highly armored "tanks" actively keep the attacking enemies focused on them,
where as "healers" ensure that the raid group remains alive until end of combat.
The final role, and the one we concern ourselves with, is "direct damagers" -
players whose job it is to deal as much damage as possible to the enemy
non-player-characters. In this role, the traditional metric by which we
distinguish good players from bad players is DPS, or "damage per second".

Like in many other games, there are sub-role classifications within these three
roles (crowd controller, disabler, hybrid heal/damage roles, etc) but these
three represent the MMO "holy trinity", and are what most end-game group content
is balanced around.

Although optimization for healing and tanking is certainly an interesting
problem, the performance of players in those roles is much harder to evalute
than the performance of a damage-dealing class.

It does not always make sense to optimize for "healing per second", as healing
is largely a zero-sum game in high level group play: there is only so much
damage going out to a group, and a cadre of healers to handle it. Healers
optimizing for pure throughput will find themselves running out of resources
towards the end of a fight; where healing generally is most important.
Additionally, not all damage is equally important to heal quickly: the relative
danger posed by a 10 damage spike when a player is at 11 health is much higher
than the same 10 damage spike when a player is at 100 health.

Similarly, tanking is not so easily quantifiable: a good tank will both maximize
their chances of staying alive while simultaneously optimizing their damage,
which poses a tradeoff in optimization space: a multi-objective problem with
directly conflicting optimization goals (tanks end up making a gearing choice
between "offensive" and "defensive" profiles). Historically, theorycrafters have
developed policies for playing tanks "defensively" as well as "offensively", but
evaluating the "goodness" of such policies is out of scope for this project.

Given the complexities mentioned above with simulation and optimization for
healers and tanks, we explicitly choose to NOT focus on these roles. Instead, we
concern ourselves in this work with the (already) herculean task of optimization
across gameplay choices for "damage dealing" roles.

Consider the following problem: for a given predetermined fight scenario with
specified / scripted enemy positions/behaviors/health/ability usage, what
choices can a damage dealer make to do as much damage as possible over the
course of that fight? Equivalently, what choices can a damage dealer make to end
a fight as quickly as possible?

## Formalization

DPS Optimization is a complicated instance of a classic planning problem, with
an interesting twist. At a very abstract high level, there are effectively two
planning components under the control of a player.  These two components are
"pre-combat player configuration" and "gameplay action policy".

The interesting twist in this problem is choices made during the planning phase
of these very separate components tend to affect optimal choices in the other
phase. For example, some action-policies favor gear-configurations with
different stat balances (maybe you have a spell that gets better and better with
more critical strike rating on your gear, so you would want to cast that spell
more frequently when you have a lot of crit rating, but almost never if you
don't) or talent choices, and some stat balances and talent choices favor
different action policies.

A very specific example of this conundrum (offline planning decision influences
online action selection) is the Assassination Rogue: there are
two "viable" approachs to playing the assassination rogue - the "poison" build
and the "bleed" build. The poison build requires a large amount of Mastery on
the rogue's gearset, because mastery increases their poison damage and not their
bleed damage. If the rogue has a random gear configuration, it's unlikely that
they would have enough mastery for the "poison" build to be good, but if they
*specifically* plan on using the poison build, they can set up a gearset with a
lot of mastery to make it better than the "bleed" build.

Another piece of complexity: both of these components (offline- and
online-planning) are thoroughly influenced by parameters of the encounter being
simulated. The theorycrafting community has traditionally constructed different
player configurations and action policies for encounters that feature multiple
persistent enemies ("AOE", or "area of effect", a term meaning an emphasis on
abilities which do damage to multiple targets at once) and encounters that
feature a single, high-health enemy ("Patchwork", named after a classical,
canonical example enemy from the Naxxramas raid).

Any new approach to automatically solving character planning + policy decision,
therefore, must take pre-fight player configuration, encounter script
parameters, and player action into account. The formalization we need to do
optimization in a high-dimensional space is direct: our goal is to maximize the
expected value of the objective function `DPS(online action policy, offline
configuration; encounter parameters)`. Note that we parameterize this function
by encounter parameters: we'll fix the encounter parameters, and attempt to
learn both a good configuration, as well as a good policy to match that
configuration for the particular encounter.

### Player Configuration

First up, players need to make choices about their character before combat ever
begins: these pre-combat choices largely consists of two components.  The first
sub-component can be thought of as "gearing/item usage" and the second
sub-component can be thought of as "talent choice" or specialization.

#### Gearing and Itemization

Players have a number of empty "inventory slots" on their character when it is
first created - these are slots like "helmet", "shoulders", "rings", "trinkets",
"weapons", etc. A complete "gear configuration" is a mapping from each slot to a
corresponding item that is equippable in that slot. Each damage specialization
is constrained to using a subset of gear: a fury warrior generally does not wear
cloth armor, and a shadow priest generally does not use a shield. Many items
have synergistic effects with other items (i.e.  item "tier sets" may impact
gameplay in major ways: traditionally, the role of "raid tier set bonuses" has
been to provide a gameplay-changing bonus to a specific player ability).

Additionally, some items (usually trinkets or weapons) have a random chance
during combat to produce a magical effect. Some items have an "on-use" ability
that provides a powerful effect whose timing is controllable by a player.

Some items (legendary items) impact the behavior of player abilities entirely,
and can provide a static on-equip effect similar in strength to player talents
(see later section for more).

Most pieces of gear provide some amount of "primary stat" (intellect, agility,
or strength) and some amount of "secondary stats" (versatility, mastery,
critical strike, or haste). Each damage dealing specialization has a different
interaction with these stats, but in combination, they form the core of player
power progression: your character gets stronger only when you get better items.

Finally, players can use non-equippable consumable items, like food, augmenting
runes, flasks, scrolls, and numerous other consumable items to get combat buffs.

Potions are a particularly impactful consumable that, due to a quirk of history,
can be chugged once right before combat begins, and then again during combat.
For clarity and historical congruence, we separate the use of a combat-potion
into the "online action policy" component, as the timing of this use is an
important player choice! The rest of the consumables (flask, runes, food,
pre-pot, etc) can safely be baked into the pre-combat "player configuration"
component.

#### Talents

Players have a choices of "talents" which further customize their
specialization. In the current implementation, characters can select one talent
from several rows of three choices each. This form of customization provides
*OUTSIZED* amounts of control on both optimal gearing as well as optimal policy
selection. Some talents grant new abilities to the player, while others modify
the behavior of existing abilities completely. Some talents are better suited to
multiple target combat, and other talents are optimized for single target
damage. Players can only choose their talents before combat starts. In practice,
some talents do not effect combat or damage meaningfully, and their impact can be
ignored. This will help prune the search space a bit.

### Encounter Configuration

An encounter is essentially a script 

### Action Policy

The core of this project involves deep reinforcement-learning on this component.

An `action policy` represents a mapping from game-state to action taken.  For
each pair of (gearing configuration, encounter parameters), we seek to identify
an optimal mapping of `state` to `action`. We formulate this problem as a
classic "Markov Decision Problem": from each state `S`, there are a number of
available actions `a`: for example, you might wait 1ms, or use ability X, use
ability Y, etc. Each of these actions `a` has a transition probability of
putting us in each of several new states `S'`. We derive reward `R` from taking
action `a` in state `S`: this is the damage value of the action at the time it
was taken.  Finally, we discount future rewards by an exponential decay
parameterized by `gamma` in order to bias the learning towards immediate
payback.

The goal of the Markov Decision Problem is to formally identify the action for
each state that maximizes the gamma-discounted reward in the limit as time goes
to infinity. This is a good match for our problem domain, and we choose to
utilize the same formulation for identifying the optimal policy.

One technique in reinforcement learning is Q-learning: rather than learn
directly "which action is optimal for this state?" and proceeding recursively
from there, an equivalent formulation is "what states are the best states to
find yourself, in general?" and choose the actions that lead to those states.
Google has famously used this technique to great effect to learn how to play the
game GO as well as top human experts.


## Architectural components / design
TODO(mrdmnd) - How do we take advantage of transfer learning?? Seems important.

### Directory Organization

```
BUILD
.clang-format
compilation\_database.json
|-proto/
  |-actor\_state.proto
  |-?
|-simulation/
  |-action.h
  |-action.cc
  |-action\_test.cc
  |-...
|-learning/
  |-?
|-tooling/
  |-bazel-compilation-database/
  |-dbc-extractor/
```

### Game Client Data parser
We need to get client data out of the game files. They're basically encoded in
weird database formats, and we need this to be usable for our simulator. This is
a non-interesting, solved problem by the SimC team, but we need this component,
unless we want to hardcode every constant for every spell and every item.

### Simulation Service

We implement an RPC service that receives SimulationRequests and produces
SimulationResponses. This is meant to be horizontally scalable. Each machine
running this service is running a simulation server binary which is heavily
multithreaded and multiprocessed. This is an embarassingly parallel problem,
with minimal data dependencies. Simulation is mostly a CPU-bound problem.

### Deep Reinforcement Learning Client

For a given gearset and encounter, this client trains a policy to handle the
encounter. It does so by using the simulation service as an oracle for the exact
DPS value of the policy.

### Constrained Gear Optimization Client

Given an encounter and a set of available gear, identify the gear, talent
composition, and optimal policy for that encounter.

### LUA Addon

This component takes the trained action policy for the given gearset and
encounter configuration, and embeds itself directly into the player's in-game
interface: it uses the existing WoW API to build up an equivalent representation
of "player state" used by the action policy network, and then runs the model
forward to determine the optimal action at any moment in time. (In practice,
we'll want to determine the optimal action at both *this instant* as well as a
few times in the future, to give the player a bit of predictive breathing room.
This is similar (almost exactly identical) to existing addons Ovale or
HeroRotation.


## Installation

### Get Started

Dependencies (some are pulled in automatically)
- [Bazel](https://bazel.build/) - you must install this.
- [CUDA](https://developer.nvidia.com/cuda-downloads) - you must install this.
- [gTest](https://github.com/google/googletest) - automatic.
- [gLog](https://github.com/google/glog) - automatic.
- [gFlags](https://github.com/gflags/gflags) - automatic.
- [gRPC](https://github.com/grpc/grpc) - automatic.
- [Benchmark](https://github.com/google/benchmark) - automatic.
- [Protobuf](https://github.com/google/protobuf) - automatic.
- [Tensorflow](https://github.com/tensorflow/tensorflow)
- C++17 compliant compiler (code built + tested with clang 5.0.1 on Arch Linux)
- CUDA-supported device

### Help Develop

Optional Dependencies for developers Linters:
- [ClangFormat](https://clang.llvm.org/docs/ClangFormat.html) - all code
  is autoformatted to Google's style guide conventions.
- [Buildifier](https://github.com/bazelbuild/buildtools) - BUILD file is
  autoformattedto Google's style guide convensions.

```
git clone  https://github.com/bazelbuild/buildtools.git
cd buildtools/
bazel build //buildifier
sudo cp bazel-bin/buildifier/buildifier /usr/bin/buildifier
```

Third-party inclusions:
- [Bazel CompilationDatabase](https://github.com/grailbio/bazel-compilation-database)
  to build Compilation Database for CLANG tooling. After bazel build, invoke
  `tools/bazel-compilation-database/generate.sh` to build compilation database.
  This seeds the rest of the useful clang completion tools.



