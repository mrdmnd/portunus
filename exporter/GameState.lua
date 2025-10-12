local UnitCastingInfo = UnitCastingInfo
local UnitChannelInfo = UnitChannelInfo
local GetSpellCooldown = GetSpellCooldown
local GetUnitSpeed = GetUnitSpeed
local UnitAffectingCombat = UnitAffectingCombat
local UnitCanAttack = UnitCanAttack
local UnitGUID = UnitGUID
local UnitHealth = UnitHealth
local UnitHealthMax = UnitHealthMax
local UnitIsDeadOrGhost = UnitIsDeadOrGhost
local UnitName = UnitName
local UnitThreatSituation = UnitThreatSituation

local mathfloor = math.floor

local game_state_frame = CreateFrame("Frame", "game_state_frame")

-- This module stores state information that gets updated either
-- a) when events fire that update this information or
-- b) when the MainLoop function to refresh the display calls into the module.

local enemy_info = {}
local combat_entry_timestamp_ms = nil
local combat_exit_timestamp_ms = nil

-- Get the spell cast information for a unit.
-- This may be nil if the unit is not casting or channeling a spell.
local function SpellCastInfoFromGUID(unit_guid)
    local token = UnitTokenFromGUID(unit_guid)
    if not token or UnitIsDeadOrGhost(token) then return nil end
    local spell_name, _, _,   spell_start_time_ms,   spell_end_time_ms, _,  spell_cast_id, spell_uninterruptible,   spell_id = UnitCastingInfo(token)
    if spell_name ~= nil then
        return {
            spell_id = spell_id,
            start_time_ms = spell_start_time_ms,
            end_time_ms = spell_end_time_ms,
            interruptible = (not spell_uninterruptible),
            channeling = false,
        }
    local channel_name, _, _, channel_start_time_ms, channel_end_time_ms, _,               channel_uninterruptible, spell_id = UnitChannelInfo(token)
    if channel_name ~= nil then
        return {
            spell_id = spell_id,
            start_time_ms = channel_start_time_ms,
            end_time_ms = channel_end_time_ms,
            interruptible = (not spell_uninterruptible),
            channeling = true,
        }
    end
    return nil
end

-- Pushes a reduced aura representation into an output table.
local function PushAura(aura, output_table)
    output_table[#output_table+1] = {
        -- Token of the unit that applied the aura.
        source_guid = UnitGUID(aura.sourceUnit),
        -- The SpellID of the aura.
        spell_id = aura.spellId,
        -- The HeroRotation function AuraRemains() is just expirationTime - GetTime()
        expiration_time_ms = mathfloor(1000*aura.expirationTime),
        -- The number of stacks of the aura.
        stacks = aura.applications,
    }
end

-- For a given enemy unit GUID, return a table of information about it.
local function EnemyInfoFromGUID(unit_guid)
    local token = UnitTokenFromGUID(unit_guid)
    if not token or UnitIsDeadOrGhost(token) then return nil end

    local helpful_auras = {}
    local harmful_auras = {}
    AuraUtil.ForEachAura(token, "HELPFUL", nil, function(aura) PushAura(aura, helpful_auras) end, true)
    AuraUtil.ForEachAura(token, "HARMFUL", nil, function(aura) PushAura(aura, harmful_auras) end, true)

    local info = {
        health_current = UnitHealth(token),
        health_maximum = UnitHealthMax(token),
        name = UnitName(token),
        in_combat = UnitAffectingCombat(token), -- returns "true or false" but the unit might not be in combat WITH YOU
        threat_situation = UnitThreatSituation("player", token), -- returns nil if `player` is not on the threat table of `token`, 0/1 if the player isnt the primary target, or 2/3 if the player is the primary target
        attackable = UnitCanAttack("player", token),
        speed = GetUnitSpeed(token), -- yards per second: running 100% speed is 7 yrd/sec, epic ground mount is 14.0 yrd/sec
        spell_cast_info = SpellCastInfoFromGUID(unit_guid),
        helpful_auras = helpful_auras,
        harmful_auras = harmful_auras,
        -- TODO: range = ...
        -- TODO: timers = ...
    }
    return info
end

local function PlayerInfo()
    local helpful_auras = {}
    local harmful_auras = {}
    AuraUtil.ForEachAura("player", "HELPFUL", nil, function(aura) PushAura(aura, helpful_auras) end, true)
    AuraUtil.ForEachAura("player", "HARMFUL", nil, function(aura) PushAura(aura, harmful_auras) end, true)

    local player_info = {
        health_current = UnitHealth("player"),
        health_maximum = UnitHealthMax("player"),
        name = UnitName("player"),
        spell_cast_info = SpellCastInfoFromGUID("player"),
        helpful_auras = helpful_auras,
        harmful_auras = harmful_auras,
    }
    return info
end



-- Unit spawns into the world (visibly)
function game_state_frame:NAME_PLATE_UNIT_ADDED(unit_id)
    local key = UnitGUID(unit_id)
    if not key then return end
    enemy_info[key] = EnemyInfoFromGUID(key)
end

-- Unit dies or is removed from the world
function game_state_frame:NAME_PLATE_UNIT_REMOVED(unit_id)
    local key = UnitGUID(unit_id)
    if not key then return end
    enemy_info[key] = nil
end

-- Entering Combat
function game_state_frame:PLAYER_REGEN_DISABLED()
    combat_entry_timestamp_ms = mathfloor(1000.0*GetTime())
end

-- Exiting Combat
function game_state_frame:PLAYER_REGEN_ENABLED()
    combat_exit_timestamp_ms = mathfloor(1000.0*GetTime())
end

function game_state_frame:UI_ERROR_MESSAGE(message_type, message)
    -- Use this to check `facing` requirements?
    -- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Events/Player.lua#L240C1-L241C1
end

game_state_frame:RegisterEvent("NAME_PLATE_UNIT_ADDED")
game_state_frame:RegisterEvent("NAME_PLATE_UNIT_REMOVED")
game_state_frame:RegisterEvent("PLAYER_REGEN_DISABLED")
game_state_frame:RegisterEvent("PLAYER_REGEN_ENABLED")
game_state_frame:RegisterEvent("UI_ERROR_MESSAGE")
game_state_frame:SetScript("OnEvent", function(self, event, ...) self[event](self, ...) end)


local function BuildSnapshotTable()
    local snapshot_time_ms = mathfloor(1000 * GetTime()) -- this field is generally valuable and we only want to do one call to GetTime to compute it per update.
    local snapshot_table = {
        snapshot_time_ms = snapshot_time_ms,
        combat_time_ms = (combat_entry_timestamp_ms ~= nil) and snapshot_time_ms - combat_entry_timestamp_ms or 0,
        player = PlayerInfo(),
        enemies = enemy_info,
    }
    return snapshot_table
end