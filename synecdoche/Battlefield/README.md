This module is intended to keep track of an evolving combat landscape.
Combat involves "units". These units might be friendly or hostile; additionally, the Player is a special unit.

In the game world, there are visible enemy nameplates corresponding to enemy targets.
The nameplate is a visual representation for the unit itself, but nameplates may fly on and off screen as the camera moves.
Nameplates can come in and off screen with the UI event NAME_PLATE_UNIT_ADDED and NAME_PLATE_UNIT_REMOVED.

All units (friendly and enemy) can be "in combat" with other units. 
The way that you determine this is via the UnitAffectingCombat() api function.
You can see who they're in combat with by looking at the UnitThreatSituation() and UnitDetailedThreatSituation() functions

You can't rely on unit nameplates disappearing to indicate unit death.
You have to actually follow the combat log event unfiltered, and track the UNIT_DIED or UNIT_DESTROYED combat log events.

There is a combat log that keeps track of "what happens" during combat, but without much positional information.

## Implementation

The `Battlefield.lua` module tracks:
- **On-screen nameplates**: Enemy nameplates currently visible
- **In-combat units**: All alive enemy units in combat (even if off-screen)
- **Mappings**: Bidirectional mapping between nameplates and units

### Key Features

1. **Event-Driven Updates**: Responds to NAME_PLATE_UNIT_ADDED, NAME_PLATE_UNIT_REMOVED, and COMBAT_LOG_EVENT_UNFILTERED
2. **Pulse Updates**: Refreshes all state every 30ms via the main update ticker
3. **Party/Raid Scanning**: Tracks what party/raid members are in combat with
4. **Stale Data Cleanup**: Removes units that haven't been seen recently
5. **Death Tracking**: Properly handles unit death via combat log events
6. **Range Tracking**: Uses LibRangeCheck-3.0 to track distance bounds (min/max) for all units

### Public API

```lua
-- Get all visible nameplates
local nameplates = SYN.Battlefield:GetNameplates()

-- Get all in-combat units (including off-screen ones)
local combatUnits = SYN.Battlefield:GetCombatUnits()

-- Get nameplate token for a specific GUID
local unitToken = SYN.Battlefield:GetNameplateForUnit(guid)

-- Get GUID for a specific nameplate token
local guid = SYN.Battlefield:GetUnitForNameplate(unitToken)

-- Get counts
local nameplateCount = SYN.Battlefield:GetNameplateCount()
local combatUnitCount = SYN.Battlefield:GetCombatUnitCount()

-- Check if unit is visible on screen
local isVisible = SYN.Battlefield:IsUnitVisible(guid)

-- Get range bounds for a specific unit
local minRange, maxRange = SYN.Battlefield:GetUnitRange(guid)

-- Get all units within a specific range (definitely within)
local closeUnits = SYN.Battlefield:GetUnitsWithinRange(40)

-- Get all units beyond a specific range (definitely beyond)
local farUnits = SYN.Battlefield:GetUnitsBeyondRange(40)

-- Check if a unit is within/beyond a range
local isClose = SYN.Battlefield:IsUnitWithinRange(guid, 10)
local isFar = SYN.Battlefield:IsUnitBeyondRange(guid, 40)

-- Get debug information
local debugInfo = SYN.Battlefield:GetDebugInfo()
```

### Usage Example

```lua
-- In your combat engine Update() function:
local combatUnits = SYN.Battlefield:GetCombatUnits()
for guid, unitData in pairs(combatUnits) do
    if not unitData.isDead then
        -- Get range information
        local rangeStr = "Unknown"
        if unitData.minRange and unitData.maxRange then
            rangeStr = string.format("%d-%d yards", unitData.minRange, unitData.maxRange)
        elseif unitData.minRange then
            rangeStr = string.format(">%d yards", unitData.minRange)
        end
        
        print(string.format("Unit %s: %d/%d HP, Range: %s, Threat: %s",
            unitData.name,
            unitData.health,
            unitData.healthMax,
            rangeStr,
            tostring(unitData.threatStatus)))
        
        -- Check if this unit is on screen
        if SYN.Battlefield:IsUnitVisible(guid) then
            local unitToken = SYN.Battlefield:GetNameplateForUnit(guid)
            -- Do something with the nameplate
        end
        
        -- Check if unit is in melee range
        if SYN.Battlefield:IsUnitWithinRange(guid, 5) then
            -- Unit is definitely within 5 yards (melee range)
        end
    end
end

-- Example: Find all enemies within 40 yards for AOE abilities
local unitsInAOERange = SYN.Battlefield:GetUnitsWithinRange(40)
local aoeTargetCount = 0
for guid, unitData in pairs(unitsInAOERange) do
    aoeTargetCount = aoeTargetCount + 1
end
print(string.format("Can hit %d enemies with AOE", aoeTargetCount))
```