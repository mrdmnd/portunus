--- ================= BATTLEFIELD STATE TRACKER =================
--- Tracks all enemy units in combat and their nameplate visibility
--- Maintains mappings between nameplates and units
--- ===============================================================

local addonName, SYN = ...

-- Localize WoW API functions
local UnitExists = UnitExists
local UnitGUID = UnitGUID
local UnitName = UnitName
local UnitAffectingCombat = UnitAffectingCombat
local UnitThreatSituation = UnitThreatSituation
local UnitDetailedThreatSituation = UnitDetailedThreatSituation
local UnitIsEnemy = UnitIsEnemy
local UnitIsDead = UnitIsDead
local UnitHealth = UnitHealth
local UnitHealthMax = UnitHealthMax
local GetTime = GetTime
local pairs = pairs
local next = next

-- LibRangeCheck
local rc = LibStub("LibRangeCheck-3.0")

-- Initialize the Battlefield module
SYN.Battlefield = SYN.Battlefield or {}
local Battlefield = SYN.Battlefield

-- State tables
local Nameplates = {}           -- [unitToken] = { guid, name, lastSeen, 
                                --                minRange, maxRange }
local CombatUnits = {}          -- [guid] = { name, inCombat, lastSeen, 
                                --            health, healthMax, 
                                --            threatStatus, isDead,
                                --            minRange, maxRange }
local NameplateToUnit = {}      -- [unitToken] = guid
local UnitToNameplate = {}      -- [guid] = unitToken

-- Event frame
local EventFrame = nil

-- Constants
local CLEANUP_INTERVAL = 5.0    -- How often to clean up stale data (seconds)
local STALE_THRESHOLD = 10.0    -- How long before we consider a unit stale
local LAST_CLEANUP = 0

--- ================= HELPER FUNCTIONS =================

-- Get the GUID for a unit token safely
local function SafeGetUnitGUID(unitToken)
    if not unitToken or not UnitExists(unitToken) then
        return nil
    end
    return UnitGUID(unitToken)
end

-- Get comprehensive unit info
local function GetUnitInfo(unitToken)
    if not unitToken or not UnitExists(unitToken) then
        return nil
    end
    
    local guid = UnitGUID(unitToken)
    local name = UnitName(unitToken) or "Unknown"
    local inCombat = UnitAffectingCombat(unitToken) or false
    local isDead = UnitIsDead(unitToken) or false
    local health = UnitHealth(unitToken) or 0
    local healthMax = UnitHealthMax(unitToken) or 1
    
    -- Threat information
    local threatStatus = UnitThreatSituation(unitToken)
    
    -- Range information using LibRangeCheck
    local minRange, maxRange = rc:GetRange(unitToken)
    
    return {
        guid = guid,
        name = name,
        inCombat = inCombat,
        isDead = isDead,
        health = health,
        healthMax = healthMax,
        threatStatus = threatStatus,
        minRange = minRange,
        maxRange = maxRange,
        lastSeen = GetTime()
    }
end

-- Check if a unit is an enemy
local function IsEnemyUnit(unitToken)
    if not unitToken or not UnitExists(unitToken) then
        return false
    end
    return UnitIsEnemy("player", unitToken)
end

--- ================= NAMEPLATE TRACKING =================

-- Add a nameplate to tracking
function Battlefield:AddNameplate(unitToken)
    if not unitToken or not UnitExists(unitToken) then
        return
    end
    
    -- Only track enemy nameplates
    if not IsEnemyUnit(unitToken) then
        return
    end
    
    local unitInfo = GetUnitInfo(unitToken)
    if not unitInfo or not unitInfo.guid then
        return
    end
    
    -- Store nameplate data
    Nameplates[unitToken] = {
        guid = unitInfo.guid,
        name = unitInfo.name,
        lastSeen = unitInfo.lastSeen,
        minRange = unitInfo.minRange,
        maxRange = unitInfo.maxRange
    }
    
    -- Update mappings
    NameplateToUnit[unitToken] = unitInfo.guid
    UnitToNameplate[unitInfo.guid] = unitToken
    
    -- Also update combat unit tracking if the unit is in combat
    if unitInfo.inCombat then
        self:UpdateCombatUnit(unitInfo)
    end
end

-- Remove a nameplate from tracking
function Battlefield:RemoveNameplate(unitToken)
    if not unitToken then
        return
    end
    
    local guid = NameplateToUnit[unitToken]
    
    -- Remove from mappings
    NameplateToUnit[unitToken] = nil
    if guid then
        UnitToNameplate[guid] = nil
    end
    
    -- Remove nameplate entry
    Nameplates[unitToken] = nil
    
    -- Note: We do NOT remove from CombatUnits here!
    -- The unit might still be alive and in combat, just off-screen
end

--- ================= COMBAT UNIT TRACKING =================

-- Update combat unit information
function Battlefield:UpdateCombatUnit(unitInfo)
    if not unitInfo or not unitInfo.guid then
        return
    end
    
    -- Store or update combat unit
    CombatUnits[unitInfo.guid] = {
        name = unitInfo.name,
        inCombat = unitInfo.inCombat,
        isDead = unitInfo.isDead,
        health = unitInfo.health,
        healthMax = unitInfo.healthMax,
        threatStatus = unitInfo.threatStatus,
        minRange = unitInfo.minRange,
        maxRange = unitInfo.maxRange,
        lastSeen = unitInfo.lastSeen
    }
end

-- Mark a unit as dead
function Battlefield:MarkUnitDead(guid)
    if not guid then
        return
    end
    
    local unit = CombatUnits[guid]
    if unit then
        unit.isDead = true
        unit.lastSeen = GetTime()
    end
end

-- Remove a dead unit from tracking
function Battlefield:RemoveDeadUnit(guid)
    if not guid then
        return
    end
    
    -- Remove from combat units
    CombatUnits[guid] = nil
    
    -- Clean up nameplate mapping if it exists
    local unitToken = UnitToNameplate[guid]
    if unitToken then
        UnitToNameplate[guid] = nil
        NameplateToUnit[unitToken] = nil
    end
end

--- ================= PARTY/RAID COMBAT TRACKING =================

-- Scan all party/raid members to find what they're in combat with
function Battlefield:ScanPartyCombatTargets()
    -- Check player
    if UnitAffectingCombat("player") then
        -- Check player's target
        if UnitExists("target") and IsEnemyUnit("target") then
            local unitInfo = GetUnitInfo("target")
            if unitInfo and unitInfo.inCombat then
                self:UpdateCombatUnit(unitInfo)
            end
        end
    end
    
    -- Check party members
    for i = 1, 4 do
        local unit = "party" .. i
        if UnitExists(unit) and UnitAffectingCombat(unit) then
            -- Check their target
            local targetUnit = unit .. "target"
            if UnitExists(targetUnit) and IsEnemyUnit(targetUnit) then
                local unitInfo = GetUnitInfo(targetUnit)
                if unitInfo and unitInfo.inCombat then
                    self:UpdateCombatUnit(unitInfo)
                end
            end
        end
    end
    
    -- Check raid members (if in raid)
    if IsInRaid() then
        for i = 1, 40 do
            local unit = "raid" .. i
            if UnitExists(unit) and UnitAffectingCombat(unit) then
                -- Check their target
                local targetUnit = unit .. "target"
                if UnitExists(targetUnit) and IsEnemyUnit(targetUnit) then
                    local unitInfo = GetUnitInfo(targetUnit)
                    if unitInfo and unitInfo.inCombat then
                        self:UpdateCombatUnit(unitInfo)
                    end
                end
            end
        end
    end
end

--- ================= PULSE UPDATE =================

-- Update all tracked nameplates and combat units
function Battlefield:PulseUpdate()
    local currentTime = GetTime()
    
    -- Update all visible nameplates
    for unitToken, plateData in pairs(Nameplates) do
        if UnitExists(unitToken) then
            local unitInfo = GetUnitInfo(unitToken)
            if unitInfo then
                -- Update nameplate timestamp and range
                plateData.lastSeen = currentTime
                plateData.minRange = unitInfo.minRange
                plateData.maxRange = unitInfo.maxRange
                
                -- Update combat unit if in combat
                if unitInfo.inCombat then
                    self:UpdateCombatUnit(unitInfo)
                end
            end
        else
            -- Nameplate unit no longer exists, remove it
            self:RemoveNameplate(unitToken)
        end
    end
    
    -- Scan party/raid for combat targets
    self:ScanPartyCombatTargets()
    
    -- Clean up stale data periodically
    if currentTime - LAST_CLEANUP > CLEANUP_INTERVAL then
        self:CleanupStaleData()
        LAST_CLEANUP = currentTime
    end
end

-- Clean up units that haven't been seen in a while
function Battlefield:CleanupStaleData()
    local currentTime = GetTime()
    local staleCutoff = currentTime - STALE_THRESHOLD
    
    -- Clean up stale combat units that are no longer in combat or are dead
    for guid, unitData in pairs(CombatUnits) do
        if unitData.lastSeen < staleCutoff then
            -- Remove if dead or not in combat
            if unitData.isDead or not unitData.inCombat then
                self:RemoveDeadUnit(guid)
            end
        end
    end
end

--- ================= EVENT HANDLERS =================

-- Handle nameplate added
local function OnNamePlateAdded(self, event, unitToken)
    Battlefield:AddNameplate(unitToken)
end

-- Handle nameplate removed
local function OnNamePlateRemoved(self, event, unitToken)
    Battlefield:RemoveNameplate(unitToken)
end

-- Handle combat log events
local function OnCombatLogEvent(self, event, ...)
    local timestamp, subevent, _, sourceGUID, sourceName, 
          sourceFlags, sourceRaidFlags, destGUID, destName, 
          destFlags, destRaidFlags = CombatLogGetCurrentEventInfo()
    
    -- Handle unit death
    if subevent == "UNIT_DIED" or subevent == "UNIT_DESTROYED" then
        Battlefield:MarkUnitDead(destGUID)
        -- Remove dead unit after a short delay
        C_Timer.After(2.0, function()
            Battlefield:RemoveDeadUnit(destGUID)
        end)
    end
    
    -- Handle combat start (any damage/healing event indicates combat)
    if subevent:find("_DAMAGE") or subevent:find("_HEAL") then
        -- If destination is an enemy, track it
        if destGUID then
            local unit = CombatUnits[destGUID]
            if unit and not unit.isDead then
                unit.lastSeen = GetTime()
                unit.inCombat = true
            end
        end
    end
end

--- ================= PUBLIC API =================

-- Get all visible nameplates
function Battlefield:GetNameplates()
    return Nameplates
end

-- Get all in-combat units
function Battlefield:GetCombatUnits()
    return CombatUnits
end

-- Get the nameplate for a GUID (if visible)
function Battlefield:GetNameplateForUnit(guid)
    return UnitToNameplate[guid]
end

-- Get the GUID for a nameplate token
function Battlefield:GetUnitForNameplate(unitToken)
    return NameplateToUnit[unitToken]
end

-- Get count of visible nameplates
function Battlefield:GetNameplateCount()
    local count = 0
    for _ in pairs(Nameplates) do
        count = count + 1
    end
    return count
end

-- Get count of in-combat units
function Battlefield:GetCombatUnitCount()
    local count = 0
    for guid, unit in pairs(CombatUnits) do
        if not unit.isDead then
            count = count + 1
        end
    end
    return count
end

-- Check if a unit is visible on screen
function Battlefield:IsUnitVisible(guid)
    return UnitToNameplate[guid] ~= nil
end

-- Get range bounds for a specific unit GUID
function Battlefield:GetUnitRange(guid)
    local unit = CombatUnits[guid]
    if unit then
        return unit.minRange, unit.maxRange
    end
    return nil, nil
end

-- Get all combat units within a specific range
-- Returns table of { guid = unitData } for units whose maxRange <= range
-- (i.e., units that are definitely within the specified range)
function Battlefield:GetUnitsWithinRange(range)
    local unitsInRange = {}
    for guid, unitData in pairs(CombatUnits) do
        if not unitData.isDead and unitData.maxRange and unitData.maxRange <= range then
            unitsInRange[guid] = unitData
        end
    end
    return unitsInRange
end

-- Get all combat units beyond a specific range
-- Returns table of { guid = unitData } for units whose minRange > range
-- (i.e., units that are definitely beyond the specified range)
function Battlefield:GetUnitsBeyondRange(range)
    local unitsBeyond = {}
    for guid, unitData in pairs(CombatUnits) do
        if not unitData.isDead and unitData.minRange and unitData.minRange > range then
            unitsBeyond[guid] = unitData
        end
    end
    return unitsBeyond
end

-- Check if a unit is definitely within a specific range
function Battlefield:IsUnitWithinRange(guid, range)
    local unit = CombatUnits[guid]
    if unit and not unit.isDead and unit.maxRange then
        return unit.maxRange <= range
    end
    return false
end

-- Check if a unit is definitely beyond a specific range
function Battlefield:IsUnitBeyondRange(guid, range)
    local unit = CombatUnits[guid]
    if unit and not unit.isDead and unit.minRange then
        return unit.minRange > range
    end
    return false
end

-- Get detailed info for debugging
function Battlefield:GetDebugInfo()
    local info = {
        nameplateCount = self:GetNameplateCount(),
        combatUnitCount = self:GetCombatUnitCount(),
        nameplates = {},
        combatUnits = {}
    }
    
    for unitToken, plateData in pairs(Nameplates) do
        info.nameplates[unitToken] = {
            guid = plateData.guid,
            name = plateData.name,
            age = GetTime() - plateData.lastSeen,
            minRange = plateData.minRange,
            maxRange = plateData.maxRange
        }
    end
    
    for guid, unitData in pairs(CombatUnits) do
        info.combatUnits[guid] = {
            name = unitData.name,
            inCombat = unitData.inCombat,
            isDead = unitData.isDead,
            health = unitData.health,
            healthMax = unitData.healthMax,
            threatStatus = unitData.threatStatus,
            minRange = unitData.minRange,
            maxRange = unitData.maxRange,
            age = GetTime() - unitData.lastSeen,
            hasNameplate = self:IsUnitVisible(guid)
        }
    end
    
    return info
end

--- ================= INITIALIZATION =================

function Battlefield:Init()
    -- Create event frame if it doesn't exist
    if not EventFrame then
        EventFrame = CreateFrame("Frame")
    end
    
    -- Register events
    EventFrame:RegisterEvent("NAME_PLATE_UNIT_ADDED")
    EventFrame:RegisterEvent("NAME_PLATE_UNIT_REMOVED")
    EventFrame:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
    
    -- Set event handler
    EventFrame:SetScript("OnEvent", function(self, event, ...)
        if event == "NAME_PLATE_UNIT_ADDED" then
            OnNamePlateAdded(self, event, ...)
        elseif event == "NAME_PLATE_UNIT_REMOVED" then
            OnNamePlateRemoved(self, event, ...)
        elseif event == "COMBAT_LOG_EVENT_UNFILTERED" then
            OnCombatLogEvent(self, event, ...)
        end
    end)
    
    -- Initialize tracking for already-visible nameplates
    for i = 1, 40 do
        local unitToken = "nameplate" .. i
        if UnitExists(unitToken) and IsEnemyUnit(unitToken) then
            self:AddNameplate(unitToken)
        end
    end
    
    print("Battlefield state tracker initialized")
end

-- Add pulse update to be called from main update loop
function Battlefield:Update()
    self:PulseUpdate()
end

return Battlefield

