import sim
from sim import *

"""
The interface for actions is pretty simple. 

An Action is a static class (not intended to be instantiated!) intended to represent the effects of using a spell or ability on an enemy target.
These classes have two methods:

In a given `sim` state, the action is either legal or illegal (due to resources, GCD constraints, not able to act while casting, etc.)
This is the `IsLegal(sim, target)` method.

Additionally, taking the action will (stochastically, possibly!) modify the `sim` object.
These state changes are done by the `TakeAction(sim, target)` method. For some actions (buffing yourself, waiting, etc), the target parameter may be none.

Note: all damaging abilities have some variance in their damage (5%) but we don't really model that; instead we'll just look at the center of the distribution.
"""


class SinisterStrike:
    def BaseDamage(sim: Sim, target: TargetState):
        ap_mod = 0.60
        spec_aura_mod = 1.05

        # Targets take 30% less damage from physical attacks, of which sinister strike is one.
        armor_mod = 0.70

        # Heavy hitter talent increases base damage of generators by 5%.
        heavy_hitter_mod = 1.05 if ("heavy_hitter" in sim.configuration.talents) else 1.00

        return ap_mod * sim.player.AttackPower() * (1 + sim.player.VersatilityRatingToPercent()) * spec_aura_mod * armor_mod * heavy_hitter_mod

    def CritRate(sim: Sim, target: TargetState):
        return 0.10 + sim.player.CritRatingToPercent() + \
            0.01*sim.configuration.talents["lethality"] + \
            0.05*(sim.configuration.talents["deadly_precision"]) + \
            1.0 * sim.player.auras["cold_blood"].duration
    
    def IsLegal(sim: Sim, target: TargetState):
        return sim.GCDReady() and sim.player.energy >= 45

    # Assumes the precondition in IsLegalFrom(state) is true.
    # IMMEDIATELY takes the spell action, cast on target at index `target` then sets all of the appropriate cooldowns and refreshes applicable auras.
    # Target index may be null for some actions that don't have targets.

    def TakeAction(sim: Sim, target: TargetState):
        reward = 0
        base_damage = SinisterStrike.BaseDamage(sim, target)

        sim.player.energy -= 45
        crit = (np.random.random() < SinisterStrike.CritRate(sim, target))
        reward += (2 if crit else SinisterStrike.BaseDamage())
        sim.player.combo_points += (2 if sim.player.auras["broadsides"].duration > 0 else 1)

        # Compute whether or not we single or double hit. Then, compute how much damage each hit does.
        p_double_hit = 0.45 + 0.25 * (1 if sim.player.auras["skull_and_crossbones"].duration > 0 else 0)
        double_hit = (np.random.random() < p_double_hit)
        if double_hit:
            crit2 = (np.random.random() < SinisterStrike.CritRate())
            reward += (SinisterStrike.CritDamage() if crit2 else SinisterStrike.BaseDamage())
            sim.player.combo_points += (2 if sim.player.auras["broadsides"].duration > 0 else 1)
            sim.player.auras["fan_the_hammer"].stacks = min(sim.player.auras["fan_the_hammer"].stacks + 3, 6)
            sim.player.auras["fan_the_hammer"].duration = 15.0

        return reward

class Ambush:
    def TakeAction(sim, target):
        pass

class PistolShot:
    def BaseDamage():
        # Opportunity is 50% less energy and 100% more damage
        # Quick draw is +1cp and 20% more damage on opportunity shot
        # Fan the hammer "undoes" the 20% more damage on the opportunity buff, so it's back to just 100% more damage w/ FTH stacks

        # If you have SPECIFICALLY opportunity, then the shot does 20% more damage.
        # If you have Fan the Hammer, this "undoes" the opportunity damage buff
        # greenskins applies to the FIRST hit (triples the damage) but not the second or third hit w/ fan the hammer
        # ALSO need to figure out if greenskin applies to all three hits...

        ap_mod = 0.376
        spec_aura_mod = 1.05

        # Targets take 30% less damage from physical attacks, of which sinister strike is one.
        armor_mod = 0.70

        # Heavy hitter talent increases base damage of generators by 5%.
        heavy_hitter_mod = 1.05 if ("heavy_hitter" in sim.configuration.talents) else 1.00

        return ap_mod * sim.player.AttackPower() * (1 + sim.player.VersatilityRatingToPercent()) * spec_aura_mod * armor_mod * heavy_hitter_mod

    def TakeAction(sim, target):
        reward = 0

        # it's either 3-2-2 combo points or 2-1-1 depending on broadsides w. triple shot
        # the second shots do 20% less damage
        cp_generated = 1 # or 2 w/ broadsides or 4 w/ triple shot or 7 w/ bs + triple shot
        quick_draw = sim.configuration.talents
        triple_shot = sim.player.auras["fan_the_hammer"].stacks > 0
        bs = sim.player.auras["broadsides"].duration > 0

        sim.player.auras["fan_the_hammer"].stacks = max(0, sim.player.auras["fan_the_hammer"].stacks - 3)
        sim.player.combo_points = min(7, sim.player.combo_points + cp_generated)

    reward = 3 * BASE_DAM if (fth > 0) else BASE_DAM

    pass

class Dispatch:

class BetweenTheEyes:
    pass

class BladeFlurry:
    pass

class AdrenalineRush:
    def IsLegal(sim, target):
        return True
    def TakeAction(sim, target):
        sim.

class SliceAndDice:
    pass

class RollTheBones:
    pass

class GhostlyStrike:
    pass

class KeepItRolling:
    pass

class Vanish:
    pass

class ShadowDance:
    pass

class EchoingReprimand:
    pass
