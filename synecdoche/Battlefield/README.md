-- This module is intended to keep track of an evolving combat landscape.
-- Here are the core objects and rules / assumptions:
-- When combat starts, there are "units" involved. These units might be friendly or hostile; additionally, the Player 
-- is a special unit.
-- There is a combat log that keeps track of "what happens" during combat, but without much positional information.
--   We're going to use this combat log for building constraints.
--   - For example, the combat log keeps track of successful spell casts and the timestamp they occurred at so you can learn that Friendly1 cast Fireball at Hostile2 at t=5 and know that Hostile2 was within Fireball range then.
--   - Another cool thing we can do is use ground-targetted AOE spells to learn that some units were within a certain radius of a fixed spot on the ground.
-- We're also able to determine a lower-bound and upper-bound range between the PLAYER and any unit at any time we want.
-- We can get the speed of any unit at any time, but not the direction.
-- We can get the target of any unit at any time. Units casting targeted spells or taking targetted actions must be "facing" their target, which means the target has to be in the 180 arc in front of the unit.
-- Enemy units can be "stationary" (like casters, or archers) or "mobile" (like melee mobs) - if enemy units DO move, they move in the direction of their target.
-- Enemy units can only move in a vector towards their target. Friendly units can move freely in any direcition.

-- Finally, to help break symmetry, we can consider the Player unit to always have their target within a +/- 20 degree arc from whey they are facing.

-- The goal here is to build a pose-estimator: where all of the units are, where they are facing, and estimates for their velocity vectors.