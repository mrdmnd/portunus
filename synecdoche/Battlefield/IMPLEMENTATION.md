# Battlefield State Tracker - Implementation Summary

## Overview

The Battlefield state tracker is a Lua module for World of Warcraft that maintains real-time awareness of all enemy units in combat, including those that have moved off-screen.

## Architecture

### Data Structures

1. **Nameplates Table**: Tracks currently visible enemy nameplates
   - Key: unitToken (e.g., "nameplate1")
   - Value: { guid, name, lastSeen }

2. **CombatUnits Table**: Tracks all alive, in-combat enemy units
   - Key: GUID (unique identifier)
   - Value: { name, inCombat, isDead, health, healthMax, threatStatus, lastSeen }

3. **Mapping Tables**: Bidirectional lookups
   - NameplateToUnit: unitToken → GUID
   - UnitToNameplate: GUID → unitToken

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
            
            if SYN.Battlefield:IsUnitVisible(guid) then
                -- Unit is on screen, can interact with nameplate
                local unitToken = SYN.Battlefield:GetNameplateForUnit(guid)
            else
                -- Unit is off-screen but still in combat
                -- Consider this for threat management, etc.
            end
        end
    end
end
```

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
- Detailed list of each with health, visibility status, and age

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

