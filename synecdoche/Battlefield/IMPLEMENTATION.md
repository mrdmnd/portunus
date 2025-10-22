# Battlefield State Tracker - Implementation Summary

## Overview

The Battlefield state tracker is a Lua module for World of Warcraft that maintains real-time awareness of all enemy units in combat, including those that have moved off-screen.

## Architecture

### Data Structures

1. **Nameplates Table**: Tracks currently visible enemy nameplates
   - Key: unitToken (e.g., "nameplate1")
   - Value: { guid, name, lastSeen, minRange, maxRange }

2. **CombatUnits Table**: Tracks all alive, in-combat enemy units
   - Key: GUID (unique identifier)
   - Value: { name, inCombat, isDead, health, healthMax, threatStatus, minRange, maxRange, lastSeen }

3. **Mapping Tables**: Bidirectional lookups
   - NameplateToUnit: unitToken → GUID
   - UnitToNameplate: GUID → unitToken

### Range Tracking

The module uses **LibRangeCheck-3.0** to estimate distance bounds for all tracked units:

- **minRange**: The minimum distance the unit could be (lower bound)
- **maxRange**: The maximum distance the unit could be (upper bound)

Range values are updated every 30ms during the pulse update. LibRangeCheck uses:
- Spell range checks (most accurate)
- Item range checks
- Interact distance checks

**Range Interpretation:**
- If both minRange and maxRange exist: unit is between minRange and maxRange yards
- If only minRange exists: unit is beyond minRange yards (out of range)
- If neither exist: range cannot be determined

### Event System

The module registers for three critical events:

1. **NAME_PLATE_UNIT_ADDED**: Fired when an enemy nameplate becomes visible
   - Adds the nameplate to tracking
   - Creates the GUID mapping
   - Adds to combat tracking if unit is in combat

2. **NAME_PLATE_UNIT_REMOVED**: Fired when a nameplate disappears
   - Removes from nameplate tracking
   - Clears the mapping
   - **Does NOT** remove from combat tracking (unit may still be alive)

3. **COMBAT_LOG_EVENT_UNFILTERED**: Monitors combat log
   - Tracks UNIT_DIED and UNIT_DESTROYED events
   - Marks units as dead
   - Schedules cleanup after 2 seconds

### Update Loop

Every 30ms (synchronized with main ticker):

1. **Nameplate Refresh**: Updates all visible nameplates
2. **Combat Scan**: Checks what party/raid members are fighting
3. **Stale Cleanup**: Every 5 seconds, removes old data

### Party/Raid Combat Scanning

The module actively scans party and raid members to find what they're in combat with:
- Checks if each party/raid member is in combat (UnitAffectingCombat)
- Examines their targets
- Adds enemy targets to combat tracking
- Ensures we track all enemies even if player hasn't targeted them

## API Usage

### Basic Queries

```lua
-- Get visible nameplates
local nameplates = SYN.Battlefield:GetNameplates()

-- Get all in-combat units (even off-screen)
local combatUnits = SYN.Battlefield:GetCombatUnits()

-- Check if a unit is on screen
if SYN.Battlefield:IsUnitVisible(guid) then
    local unitToken = SYN.Battlefield:GetNameplateForUnit(guid)
    -- Work with the nameplate
end
```

### Combat Engine Integration

```lua
function MyCombatEngine:Update()
    local combatUnits = SYN.Battlefield:GetCombatUnits()
    
    for guid, unitData in pairs(combatUnits) do
        if not unitData.isDead then
            -- This is a live enemy in combat
            
            -- Check range for ability decisions
            if unitData.maxRange and unitData.maxRange <= 40 then
                -- Definitely within 40 yards - can use ranged abilities
            end
            
            if SYN.Battlefield:IsUnitVisible(guid) then
                -- Unit is on screen, can interact with nameplate
                local unitToken = SYN.Battlefield:GetNameplateForUnit(guid)
            else
                -- Unit is off-screen but still in combat
                -- Consider this for threat management, etc.
            end
        end
    end
    
    -- Example: AOE targeting decisions
    local closeTargets = SYN.Battlefield:GetUnitsWithinRange(8)
    if next(closeTargets) then
        -- Multiple enemies in melee range, consider AOE rotation
    end
end
```

### Range API Functions

The module provides several convenience functions for range-based queries:

1. **GetUnitRange(guid)**: Returns minRange, maxRange for a specific unit
2. **GetUnitsWithinRange(range)**: Returns all units whose maxRange ≤ range
3. **GetUnitsBeyondRange(range)**: Returns all units whose minRange > range  
4. **IsUnitWithinRange(guid, range)**: Returns true if unit's maxRange ≤ range
5. **IsUnitBeyondRange(guid, range)**: Returns true if unit's minRange > range

These functions are useful for:
- AOE ability targeting (how many enemies in range?)
- Melee vs ranged ability decisions
- Positioning and kiting logic
- Threat management for tanks

### Debugging

Use the slash command in-game:
```
/syn battlefield
```
or
```
/syn bf
```

This will print:
- Count of visible nameplates
- Count of in-combat units
- Detailed list of each with health, range bounds, visibility status, and age

Example output:
```
=== Battlefield State ===
Visible Nameplates: 3
Combat Units: 5

Nameplates:
  nameplate1: Training Dummy [5-10 yds] (0.1s ago)
  nameplate2: Target Dummy [15-20 yds] (0.1s ago)
  nameplate3: Practice Dummy [30-40 yds] (0.1s ago)

Combat Units:
  Training Dummy: 1000/1000 HP [5-10 yds] [VISIBLE] [COMBAT]  (0.1s ago)
  Target Dummy: 1000/1000 HP [15-20 yds] [VISIBLE] [COMBAT]  (0.1s ago)
  Practice Dummy: 1000/1000 HP [30-40 yds] [VISIBLE] [COMBAT]  (0.1s ago)
  Distant Enemy: 500/1000 HP [>40 yds] [OFF-SCREEN] [COMBAT]  (2.3s ago)
  Boss Enemy: 50000/100000 HP [10-15 yds] [OFF-SCREEN] [COMBAT]  (0.5s ago)
```

## Key Design Decisions

1. **Separate Tracking**: Nameplates and combat units are tracked separately
   - Nameplates can disappear while units remain alive
   - Units can be in combat before/after nameplate visibility

2. **Stale Data Management**: 
   - Units not seen for 10+ seconds are candidates for cleanup
   - Only removed if also marked as dead or out of combat
   - Prevents memory leaks during long combat sessions

3. **Death Handling**:
   - Mark dead immediately via combat log
   - Wait 2 seconds before full cleanup
   - Allows for death animations and effects

4. **Threat Tracking**:
   - Stores UnitThreatSituation results
   - Can be used for tank awareness, target prioritization

## Performance Considerations

- Event handlers are lightweight (just table operations)
- Pulse update scans ~40 nameplates + party/raid members
- Cleanup runs every 5 seconds (not every frame)
- All lookups are O(1) via hash tables

## Future Enhancements

Potential additions:
- Position tracking (requires external addons or estimated positioning)
- Cooldown tracking for specific enemy abilities
- Interrupt coordination for party/raid
- Historical combat data for analysis

