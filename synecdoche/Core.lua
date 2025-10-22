--- ============================ HEADER ============================
--- ======= LOCALIZE =======
-- Addon
local addonName, SYN = ...
-- Lua
local print         = print
-- File Locals


--- ======= GLOBALIZE =======
SYN.MAXIMUM = 40 -- Max # Buffs and Max # Nameplates.

--- ======= UNIT =======
do
    local Unit = Class()
    SYN.Unit = Unit
    function Unit:New(UnitID)
        if type(UnitID) == "string" then error("UnitID must be a string") end
        self.UnitID = UnitID
        self.Init()
    end

    local UnitGUIDMap = {}
    SYN.UnitGUIDMap = UnitGUIDMap
    function Unit:RemoveUnitGUIDMapEntry()
        if UnitGUIDMap[self.UnitGUID] and UnitGUIDMap[self.UnitGUID][self.UnitID] then
            UnitGUIDMap[self.UnitGUID][self.UnitID] = nil;
            if next(UnitGUIDMap[self.UnitGUID]) == nil then
                UnitGUIDMap[self.UnitGUID] = nil;
            end
        end
    end

    function Unit:AddUnitGUIDMapEntry()
        if not self.UnitGUID or not self.UnitID then return end
        if not UnitGUIDMap[self.UnitGUID] then
            UnitGUIDMap[self.UnitGUID] = {}
        end
        if not UnitGUIDMap[self.UnitGUID][self.UnitID] then
            UnitGUIDMap[self.UnitGUID][self.UnitID] = self;
        end
    end

    function Unit:Init()
        self:RemoveUnitGUIDMapEntry()
        self.UnitGUID = nil
        self.UnitNPCID = nil
        self.UnitName = nil
        self.UnitExists = false
        self.UnitCanBeAttacked = false
    end

    -- "Special" unit pointers
    Unit.Player = Unit("player")
    Unit.Pet = Unit("pet")
    Unit.Target = Unit("target")
    Unit.Focus = Unit("focus")
    Unit.MouseOver = Unit("mouseover")
    Unit.Vehicle = Unit("vehicle")

    -- Iterable units
    local UnitIDs = {
        -- Type,        Count
        { "Arena",      5,          },
        { "Boss",       4,          },
        { "Nameplate",  SYN.MAXIMUM },
        { "Party",      4,          },
        { "Raid",       40,         }
    }
    for _, UnitID in pairs(UnitIDs) do
        local UnitType = UnitID[1]
        local UnitCount = UnitID[2]
        Unit[UnitType] = {}
        for i = 1, UnitCount do
            local UnitKey = stringformat("%s%d", UnitType, i)
            Unit[UnitType[UnitKey:lower()]] = Unit(UnitKey)
        end
    end
end


--- ======= SPELL =======


--- ======= ITEM ========


end