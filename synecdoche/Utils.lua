--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
local tableinsert = table.insert
local setmetatable = setmetatable

-- File Locals
local Utils = {}

--- ======== GLOBALIZE =========
-- Addon
SYN.Utils = Utils

--- ================ CONTENTS ================

function BooleanToInt(value)
    return value and 1 or 0
end

function IntToBoolean(value)
    return value ~= 0
end

function ValueInTable(table, value)
    for _, v in pairs(table) do
        if v == value then
            return true
        end
    end
    return false
end

function ValueInArray(array, value)
    for i = 1, #array do
        if array[i] == value then
            return true
        end
    end
    return false
end

function FindValueInArray(array, value)
    for i = 1, #array do
        if array[i] == value then
            return i
        end
    end
    return nil
end

function MergeTableValues(T1, T2)
    local result = {}
    for k, v in pairs(T1) do
        tableinsert(result, v)
    end
    for k, v in pairs(T2) do
        tableinsert(result, v)
    end
    return result
end

function MergeTableByKey(T1, T2)
    local result = {}
    for k, v in pairs(T1) do
        result[k] = v
    end
    for k, v in pairs(T2) do
        result[k] = v
    end
    return result
end

--- ======= PSEUDO-CLASS =======
function Class()
  local Class = {}
  Class.__index = Class
  setmetatable(Class, {
    __call =
    function(self, ...)
      local Object = {}
      setmetatable(Object, self)
      Object:New(...)
      return Object
    end
  })
  return Class
end


function SynDebug(...)
    print("[|cFFFF6600Synecdoche DEBUG|r]", ...)
end

function SynInfo(...)
    print("[|cFFFF6600Synecdoche INFO|r]", ...)
end

function SynWarn(...)
    print("[|cFFFF6600Synecdoche WARN|r]", ...)
end

function SynError(...)
    print("[|cFFFF6600Synecdoche ERROR|r]", ...)
end

--- ======= SPELL PSEUDOCLASS =======
-- Represents a castable spell/ability
SYN.Spell = Class()
function SYN.Spell:New(data)
    self.id = data.id or 0
    self.name = data.name or "Unknown Spell"
    self.icon = data.icon or 134400 -- Default icon
    self.minRange = data.minRange or 0
    self.maxRange = data.maxRange or 0
    self.cooldown = data.cooldown or 0
    self.charges = data.charges or nil
    self.cost = data.cost or 0
    self.costType = data.costType or nil -- e.g., "Maelstrom", "Mana"
    self.castTime = data.castTime or 0 -- 0 for instant cast
    self.gcd = data.gcd or 1.5 -- Global cooldown in seconds
    self.replaces = data.replaces or nil -- Spell ID that this replaces
    self.replacedBy = data.replacedBy or nil -- Spell ID that replaces this
    
    -- Optional metadata
    self.school = data.school or nil -- "Fire", "Frost", "Nature", etc.
    self.category = data.category or nil -- "Damage", "Heal", "Utility"
    self.tags = data.tags or {} -- Array of tags like "AoE", "SingleTarget"
end

function SYN.Spell:GetID()
    return self.id
end

function SYN.Spell:GetName()
    return self.name
end

function SYN.Spell:GetIcon()
    return self.icon
end

function SYN.Spell:GetRange()
    return self.minRange, self.maxRange
end

function SYN.Spell:IsInRange(unitToken)
    if self.maxRange == 0 then
        return true -- Melee or no range restriction
    end
    -- Use LibRangeCheck if available
    local rc = LibStub and LibStub("LibRangeCheck-3.0", true)
    if rc then
        local minRange, maxRange = rc:GetRange(unitToken)
        if maxRange and maxRange <= self.maxRange then
            return true
        end
    end
    return false
end

function SYN.Spell:HasTag(tag)
    for _, t in ipairs(self.tags) do
        if t == tag then
            return true
        end
    end
    return false
end

--- ======= AURA PSEUDOCLASS =======
-- Represents a buff or debuff
SYN.Aura = Class()
function SYN.Aura:New(data)
    self.id = data.id or 0
    self.name = data.name or "Unknown Aura"
    self.icon = data.icon or 134400 -- Default icon
    self.duration = data.duration or 0 -- Base duration in seconds (0 = permanent)
    self.maxStacks = data.maxStacks or 1
    self.type = data.type or "debuff" -- "buff" or "debuff"
    self.pandemic = data.pandemic or false -- Can be boolean or number (seconds)
    self.pandemicWindow = data.pandemicWindow or 0.3 -- Default 30% pandemic
    self.dispelType = data.dispelType or nil -- "Magic", "Curse", "Disease", "Poison"
    self.fromPlayer = data.fromPlayer or true -- Only track if cast by player
    
    -- Optional metadata
    self.school = data.school or nil -- "Fire", "Frost", "Nature", etc.
    self.category = data.category or nil -- "Damage", "Control", "Defensive"
    self.priority = data.priority or 0 -- Higher priority = more important to track
    self.tags = data.tags or {} -- Array of tags
end

function SYN.Aura:GetID()
    return self.id
end

function SYN.Aura:GetName()
    return self.name
end

function SYN.Aura:GetIcon()
    return self.icon
end

function SYN.Aura:GetDuration()
    return self.duration
end

function SYN.Aura:IsPandemic()
    return self.pandemic ~= false
end

function SYN.Aura:GetPandemicWindow()
    if type(self.pandemic) == "number" then
        return self.pandemic
    elseif self.pandemic then
        return self.duration * self.pandemicWindow
    end
    return 0
end

function SYN.Aura:CanRefresh(remaining)
    if not self:IsPandemic() then
        return false
    end
    local pandemicWindow = self:GetPandemicWindow()
    return remaining <= pandemicWindow
end

function SYN.Aura:IsBuff()
    return self.type == "buff"
end

function SYN.Aura:IsDebuff()
    return self.type == "debuff"
end

function SYN.Aura:HasTag(tag)
    for _, t in ipairs(self.tags) do
        if t == tag then
            return true
        end
    end
    return false
end

SYN.MAXIMUM = 40 -- Max # Buffs and Max # Nameplates.