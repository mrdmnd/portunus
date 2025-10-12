- want a list of specs and their IDs like https://github.com/herotc/hero-lib/blob/thewarwithin/HeroLib/Core.lua#L56
- determine if you are in an "encounter" with an encounterID like a boss fight, or just regular combat
- determine if there's a priority mob
- allocate defensive cooldowns if (you're going to die, or, if you would be less than some health buffer after an attack)
- determine time to die with "confidence score" (very confident it is X seconds to die, less confident that it is Y seconds to die, etc)
- special case mobs that don't die at zero or have shields or take extra damage or other weird circumstances
- want distance to target
- want in flight spells to target
- want impact time for spells in flight to target
- want if target is moving or not
- want bounded distance for all units (hard math problem?)

- figure out if we should register for PLAYER_TARGET_CHANGED or just check the current target
  - what is PLAYER_SOFT_ENEMY_CHANGED

- register for nameplate updates? or just check current ones in a loop (probably events?)

- architecture:
- once per ... second? we do a full scan of the environment
- between full scans, we do the periodic event tracking? to prevent us from getting out of sync with the environment?
- clear enemies that die from the game state

- need to keep track of enemies that are on screen AS WELL as any enemies that are NOT on screen (remove them when they die)
- track "should stop casting" events?
- need good POWER estimation into the future (next global, global after that if you press the button we suggest, etc)

-- TODO / thoughts
-- Handle training dummies appropriately.
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Class/Unit/Main.lua#L160

-- Determine our point-in-time power regeneration:
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Class/Unit/Power.lua#L46

-- Range checking is gonna be a Pretty Hard Problem to tackle.
-- https://github.com/herotc/hero-lib/blob/dragonflight/HeroLib/Class/Unit/Range.lua

-- It's not clear if we really need TTD tracking at all but maybe we do.
-- https://github.com/herotc/hero-lib/blob/dragonflight/HeroLib/Class/Unit/TimeToDie.lua

-- Implement a "player inspector" that fires once when we zone in or change static stuff, etc.
-- Don't put this call in the ReadFullGameState() function.
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Events/Player.lua#L133

-- Implement a tracker for last-spell-cast events that allow us to know if a spell is in the air or not.

-- Implement a tracker for premultiplier when targets are dotted with snapshot dots.

-- Implement event tracking system based on big-wigs timers.

-- Handle what happens when units go off screen but aren't dead (camera pan).

-- Implement persistent data (talent config, etc)

-- Implement stat tracking

-- SpellInFlight, CurrentTarget, stuff like that?, CombatStarted, CombatEnded, etc. Miscellaneous

-- TODO: determine if the nameplate is "in front" of the character for spells that care about that
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Events/Player.lua#L239
-- would have to do something weird like "keep track of the thing we indeded to hit with damage then see if we.. did"

-- TODO: track DR status by listening to CC events

