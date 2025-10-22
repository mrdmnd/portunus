# Range Tracking Feature

## Overview

The Battlefield module now tracks distance bounds (min/max range) for all tracked units using LibRangeCheck-3.0.

## What Was Added

### Data Structure Changes

Both `Nameplates` and `CombatUnits` tables now include:
- `minRange`: Lower bound of distance estimate (in yards)
- `maxRange`: Upper bound of distance estimate (in yards)

### New API Functions

1. **`GetUnitRange(guid)`**
   - Returns: `minRange, maxRange`
   - Get distance bounds for a specific unit

2. **`GetUnitsWithinRange(range)`**
   - Returns: Table of units whose `maxRange <= range`
   - Units that are definitely within the specified range
   - Useful for: AOE targeting, melee ability decisions

3. **`GetUnitsBeyondRange(range)`**
   - Returns: Table of units whose `minRange > range`
   - Units that are definitely beyond the specified range
   - Useful for: Range checks, kiting decisions

4. **`IsUnitWithinRange(guid, range)`**
   - Returns: `true` if unit's `maxRange <= range`
   - Quick check if a specific unit is within range

5. **`IsUnitBeyondRange(guid, range)`**
   - Returns: `true` if unit's `minRange > range`
   - Quick check if a specific unit is beyond range

## How LibRangeCheck Works

LibRangeCheck-3.0 uses multiple methods to estimate range:
- **Spell range checks**: Most accurate, uses known spell ranges
- **Item range checks**: Uses items with known interaction ranges
- **Interact distance checks**: Fallback for special cases

The library returns a range bracket (min-max) rather than exact distance.

## Range Interpretation

### Both minRange and maxRange exist
```lua
minRange = 10, maxRange = 20
-- Unit is between 10 and 20 yards away
```

### Only minRange exists
```lua
minRange = 40, maxRange = nil
-- Unit is beyond 40 yards (out of max detection range)
```

### Neither exists
```lua
minRange = nil, maxRange = nil
-- Range cannot be determined (unit may be invalid or dead)
```

## Common Use Cases

### AOE Ability Targeting

```lua
-- How many enemies can I hit with my 8-yard AOE?
local targets = SYN.Battlefield:GetUnitsWithinRange(8)
local count = 0
for _ in pairs(targets) do count = count + 1 end

if count >= 3 then
    -- Use AOE rotation
else
    -- Use single-target rotation
end
```

### Melee vs Ranged Decisions

```lua
local guid = UnitGUID("target")
if SYN.Battlefield:IsUnitWithinRange(guid, 5) then
    -- In melee range, use melee abilities
else
    -- Use ranged abilities
end
```

### Positioning and Kiting

```lua
-- Count how many enemies are too close
local dangerouslyClose = SYN.Battlefield:GetUnitsWithinRange(10)
local closeCount = 0
for _ in pairs(dangerouslyClose) do closeCount = closeCount + 1 end

if closeCount > 2 then
    -- Too many enemies nearby, consider repositioning
end
```

### Ability Range Checks

```lua
-- Check if target is in range for a 40-yard spell
local targetGuid = UnitGUID("target")
local minRange, maxRange = SYN.Battlefield:GetUnitRange(targetGuid)

if maxRange and maxRange <= 40 then
    -- Definitely in range, cast spell
elseif minRange and minRange > 40 then
    -- Definitely out of range, don't try to cast
else
    -- Uncertain, try to cast and handle failure
end
```

## Performance

Range checks happen every 30ms during the pulse update:
- LibRangeCheck uses internal caching to minimize overhead
- Range data is stored alongside other unit data (no extra lookups)
- All range queries are O(1) or O(n) where n = number of combat units

## Debugging

Use `/syn bf` to see range information for all tracked units:

```
Combat Units:
  Training Dummy: 1000/1000 HP [5-10 yds] [VISIBLE] [COMBAT]  (0.1s ago)
  Boss Enemy: 50000/100000 HP [>40 yds] [OFF-SCREEN] [COMBAT]  (2.3s ago)
```

Range is displayed in brackets:
- `[5-10 yds]` = between 5 and 10 yards
- `[>40 yds]` = beyond 40 yards
- `[Unknown]` = range cannot be determined

## Integration Notes

Range tracking is automatically updated:
- When nameplates are added/removed
- During every pulse update (30ms)
- For all combat units, even off-screen ones

No additional initialization required - just use the API functions.

