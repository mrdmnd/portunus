import numpy as np
import bisect
from queue import PriorityQueue

"""
High level architecture overview:
- We have a "simulation" object that takes as input a static player configuration (stats, talents, gear, etc) as well as a static encounter specification (patchwork, dslice, etc).
    - Note: currently our encounter specification is just 1T patchwork for 300s (5m).
- Our simulation object (`sim`) maintains some state about the player (sim.player) as well as some state about each of the valid targets (sim.[targets]).
- The simulation object also maintains an event queue, as well as a global "sim time" scalar.
- The player object maintains power resources like combo points and energy as well as two generalized trackers: cooldowns, and auras.
    - A `cooldowns` object is a dict that maps spells to (cooldown) scalars.
    - An `auras` object is a dict that maps spells to (duration, stacks) tuples.
- The sim.player has both `cooldowns` and `auras`, whereas each sim.target just has an `auras` object.

- The sim has a few "advance" methods, which work through the event queue and update resources/cooldowns/auras as necessary.
- Some of these are internal-use-only.
    - AdvanceThroughNextEvent()
    - AdvanceToDecisionPoint()

- Architecturally, we assume the simulation is "atomically split" by the events in the queue; that is, 
- it is always safe to assume nothing will randomly proc between the event at the top of the queue and the second-from-top while we are processing the top event.
- This allows us to do things like "have a proc update our haste" and then fast forward to the next event and linearly interpolate the energy gain based on the haste from the first event
- all the way up until the second event fires.

- additionally, we accumulate reward PER TIMESTEP, which means it's the immediate reward of taking the action we specify PLUS the damage we've already got running in the "background" - autoattacks, dot ticks, etc.
- in order for the sim to clearly assign reward 


What IS a timestep, here? Is it the period between taking one action and taking another? I think it makes the most sense to have this decision be "the time between AdvanceToDecisionPoint() calls".
This makes some sense to me because it captures the notion of the shortest atom of time it makes sense to wait to ask the player for input.

When IS a decision point? In SIMC, this corresponds to something like the notion of when an actor becomes ready().
On a rogue, a decision point is forced whenever a) the GCD comes off CD
"""

class Sim():
    def __init__(self):
        self.time = 0.0
        self.event_queue = PriorityQueue()
        self.configuration = PlayerConfig()
        self.encounter = EncounterSpecification()
        self.player = PlayerState(self.configuration)

        # Note: we'll have to figure out how to handle the design decision of "some targets become available mid encounter."
        self.targets = [TargetState(0)]

    # Advances the state of the simulation until the next scheduled event callback.
    # Advance the clock, then process the event in the queue, then remove it from the queue (scheduling additional callbacks as necessary).
    def AdvanceThroughNextEvent(self):
        event = self.event_queue.get()
        time_delta = event.scheduled_time - self.time
        passive_reward = 0

        # do common sim-advancey-stuff
        # Roll auras down by time_delta seconds
        # Roll cooldowns down by time_delta seconds
        # Tick dots down by time_delta seconds (this might yields some reward!) (needs to be implemented)
        # Regenerate energy up by time_delta seconds of regen, using current haste value (as we assume the simulation is atomically split by these events)

        # actually process the event in the queue
        event.callback(self)
        return passive_reward

    def AdvanceFixedTimestep(self, timestep):
        pass

    def AdvanceToEnergy(self, energy_threshold):
        pass

    # Advances the sim through events until the player is ready to take their next action: either the GCD is zero, or the GCD is non-zero but there are available off-gcd actions.
    # Additionally, if the player does not have enough energy available?
    # What happens if the previous action was "wait until next decision point" - does that wait until the next spell comes off cooldown? Does it wait until the resource being pooled caps out?
    # At a certain point we need to force the player to act - you cannot wait around forever.
    def Advance():
        pass

# Scheduled time is the time that this event is slated to fire.
# Callback is a function that takes a Sim() object and is allowed to mutate said object (even adding new events in the event queue.)
class Event():
    def __init__(self, scheduled_time, callback):
        self.scheduled_time = scheduled_time
        self.callback = callback

# Static config stuff from character setup. Eventually this will handle talents/gear/enchants/etc.
class PlayerConfig():
    def __init__(self):
        self.stamina = 44591
        self.mainhand_dps = 533.3
        self.offhand_dps = 533.3

        self.base_agility = 12434
        self.base_crit_rating = 4539
        self.base_haste_rating = 1150
        self.base_versatility_rating = 8925
        self.base_mastery_rating = 749

        # A map of talents (by name) that the player may have selected to their rank.
        self.talents = {}

    # Attack speed scales multiplicatively w/ AR, slice and dice, and haste: (1.2) * (1.5) * (1 + HastePercent())

# To be implemented later; this will eventually describe the encounter. Right now, we'll train on 1T 5m Patchwork.
class EncounterSpecification():
    def __init__(self):
        pass

# This class holds data about an individual target.
class TargetState():
    def __init__(self, id):
        self.id = 0
        self.max_health = 1000000
        self.current_health = 1000000
        self.auras = {}

# This class holds data about the player's state.
class PlayerState():
    def __init__(self, config: PlayerConfig):
        self.initial_config = config
        self.energy = 100.0
        self.combo_points = 0
        self.player_gcd_base = 1.0
        self.agility = config.base_agility
        self.crit_rating = config.base_crit_rating
        self.haste_rating = config.base_haste_rating
        self.versatility_rating = config.base_versatility_rating
        self.mastery_rating = config.base_mastery_rating
        self.auras = {}
        self.cooldowns = {}

    def AttackPower(self):
        return self.agility + 6.0 * self.initial_config.mainhand_dps
    
    def OHAttackPower(self):
        return (self.agility + 6.0 * self.initial_config.offhand_dps) / 2.0

    # Internal method to handle stat DRs.
    # DRs are of the (0%, 10%, 20%, 30%, 40%, 50%, 100%) cutoff:
    def _RatingToPercent(rating: int, scale: int, cutoffs: list[int]) -> float:
        deltas = [cutoffs[i+1] - cutoffs[i] for i in range(len(cutoffs)-1)]
        multipliers = [1.0, 0.9, 0.8, 0.7, 0.6, 0.5]
        effective_rating = 0
        rating = min(rating,  cutoffs[-1])
        for (delta, mult) in zip(deltas, multipliers):
            if rating > delta:
                effective_rating += delta * mult
                rating -= delta
            else:
                effective_rating += rating * mult
                break

        return effective_rating / (100.0 * scale)

    def VersatilityRatingToPercent(self):
        return self._RatingToPercent(self.versatility_rating, 205, [0, 6150, 8200, 10250, 12300, 16400, 41000])
    def CritRatingToPercent(self):
        return self._RatingToPercent(self.crit_rating, 180, [0, 5400, 7200, 9000, 10800, 14400, 36000])
    def HasteRatingToPercent(self):
        return self._RatingToPercent(self.haste_rating, 170, [0, 5100, 6800, 8500, 10200, 13600, 34000])
    def MasteryRatingToMasteryPoints(self):
        return self._RatingToPercent(self.mastery_rating, 180, [0, 5400, 7200, 9000, 10800, 14400, 36000])
    def MasteryPointsToMasteryPercent(self):
        # Outlaw is 2.9% AP coefficient to maingauche per mastery point
        return 2.9 * self.MasteryRatingToMasteryPoints()

    def GCDReady(self):
        return self.player.cooldowns["GCD"] == 0.0
    


# Testing spot
if __name__ == "__main__":
    sim = Sim()
    policy = Policy()
    reward = 0

    while True:
        chosen_action = policy.ChooseAction(sim)
        immediate_reward = sim.TakeAction(chosen_action)
        delayed_reward = sim.Advance()
        reward += (immediate_reward + delayed_reward)



