--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
local tableinsert = table.insert

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

SYN.MAXIMUM = 40 -- Max # Buffs and Max # Nameplates.