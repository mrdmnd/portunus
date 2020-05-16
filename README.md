# Portunus

[![Build Status](https://travis-ci.org/mrdmnd/portunus.svg?branch=master)](https://travis-ci.org/mrdmnd/portunus)

[Current WebUI](http://portunuspl.us)

Portunus is the Roman god of keys and gates - an appropriate name for a tool that attempts to automatically optimize high-end Mythic Keystone runs in World of Warcraft!

Given a group of players, a set of "control policies" for those players, and a dungeon route configuration, Portunus simulates running through the entire key and returns a "final time" for your group to shoot for.
We allow the user to specify either a standard, deterministic control policy a la a traditional SIMC APL, or a pre-trained control policy in the form of a tensorflow model.

## Background

See [Background](BACKGROUND.md) for the motivation behind Portunus.

## Components

The goal is for Portunus to ultimately contain a few core components: their development priority is listed in order.

(1) A networked, distributed, RPC simulation service, for evaluating encounter configurations against a simulation engine (sort of like SIMC-lite). This component is intended to scale horizontally, and intended to be run behind a load balancer in a cloud computing environment.

(2) A web UI front end for configuration (much like Method Dungeon Tools) that allow users to set up a dungeon encounter configuration that gets run by component (1) (group composition, gearing, monster pulls, CC, group skips, travel time, bloodlust + cooldown assignments, etc). Ideally, this configuration is exportable into MethodDungeonTools for on-the-fly visualization in-game of the intended route, as well as auto-generate notes indicating CD assignments and likely time-to-completion timestamps for each trash pack.

This frontend should also provide a "debug" view for a single iteration of the simulator (much like warcraft logs) that provides timeline information, group cooldown usage, buff uptime, damage taken/done/etc.

(3) A GPU-powered reinforcement learning module (built on TensorFlow) that trains a deep neural network to evaluate a (state, action) Q-function, and performs policy gradient descent to iteratively improve our control policies for each class.

(4) An addon for WOW much like HeroRotation that reads the world state and runs the appropriate policy model to determine the next action for a player to take.

## Installation

Portunus has several components. The core simulation service is built with Rust, and requires a full rust toolchain setup.
The web UI front end is built with python3 and flask. We assume a modern 64-bit architecture running some flavor of linux.

## Developer Guide

### Building Portunus

To build the simulation service, run ...

This starts up a local RPC service on port 50051 by default (configurable).

### Running Portunus

To run the simulation service, invoke ...

### Training A New Control Policy

To run policy generation, invoke

...

where `$ENCOUNTER_CONFIG_PATH` contains details on the raid encounter and `$CHARACTER_CONFIG_PATH` contains details on the player character (gear, etc).

## Portunus Architecture Overview

### Simulation Service

We implement an RPC service that receives SimulationRequests and produces SimulationResponses. This is meant to be horizontally scalable. Each machine running this service is running a multithreaded simulation server binary. This is an embarassingly parallel problem, with minimal data dependencies. Simulation is mostly a CPU-bound problem, although it might be an interesting problem to attempt discrete event simulation on GPU.

To actually build the simulation, we need to get client data out of the game files. They're encoded in weird database formats, and we need this to be usable for our simulator. This is a non-interesting, solved problem by the SimC team, but we need this component, unless we want to hardcode every constant for every spell and every item. We rely on this data being available at compile time in the simulation engine code.

### Web Front End

Portunus is designed to be used from a web front end. We provide a web UI that allows users to set up their configuration (classes/specs/talents/gear) as well as their intended pull plan. We'll provide information on "efficiency" in the UI like how much total mob HP is being pulled relative to mob count; what the absolute minimum required group DPS is to chest the key at the given level, and whether or not their route is theoretically possible assuming perfect play.

The UI will allow users to specify pulls, skips, CC, death runs, when CDs such as lust are used, when to use potions, and other encounter-specific things. We'll also provide a nice visualization a la warcraft logs of a single iteration of the simulator, showing what uptime people get on various buffs, when they should press their buttons, etc. 

### Deep Reinforcement Learning Client

The learning client takes in a configuration describing the encounter to be simulated, as well as a configuration for the player character. This component feeds simulation configurations to the simulation service to evaluate the quality of an action policy. We learn an optimal policy for the given encounter using deep Q learning. Finally, we return the (gearset, policy) pair that performed the best.

### LUA Addon

This component takes the trained action policy for the given gearset and encounter configuration, and embeds itself directly into the player's in-game interface: it uses the existing WoW API to build up an equivalent representation of "player state" used by the action policy network, and then runs the model forward to determine the optimal action at any moment in time. In practice, we'll want to determine the optimal action at both _this instant_ as well as a few times in the future, to give the player a bit of predictive breathing room. This is similar (almost exactly identical) to existing addons Ovale or HeroRotation.
