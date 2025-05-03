local GameStateSnapshotter = {}


local Aura = Portunus.Modules["game_state_schema.Aura"]
local Cooldown = Portunus.Modules["game_state_schema.Cooldown"]
local EnemyUnit = Portunus.Modules["game_state_schema.EnemyUnit"]
local GameState = Portunus.Modules["game_state_schema.GameState"]
local SpellCastInfo = Portunus.Modules["game_state_schema.SpellCastInfo"]
local PlayerUnit = Portunus.Modules["game_state_schema.PlayerUnit"]
local Resource = Portunus.Modules["game_state_schema.Resource"]
local ResourceType = Portunus.Modules["game_state_schema.ResourceType"]
local UnitBase = Portunus.Modules["game_state_schema.UnitBase"]

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

local function HandleAura(aura, output_table)
    output_table[#output_table+1] = {
        source_guid = UnitGUID(aura.sourceUnit), -- Token of the unit that applied the aura.
        spell_id = aura.spellId,
        expires = mathfloor(1000*aura.expirationTime), -- AuraRemains is just expirationTime - GetTime()
        stacks = aura.applications,
    }
end

local function SpellCastInfoFromGUID(unit_guid)
    local token = UnitTokenFromGUID(unit_guid)
    if not token or UnitIsDeadOrGhost(token) then return nil end
    local spell_name, _, _,   spell_start_time_ms,   spell_end_time_ms, _,  spell_cast_id, spell_uninterruptible,   spell_id = UnitCastingInfo(token)
    local channel_name, _, _, channel_start_time_ms, channel_end_time_ms, _,               channel_uninterruptible, spell_id = UnitChannelInfo(token)
    if spell_name ~= nil then
        return {
            spell_id = spell_id,
            start_time_ms = spell_start_time_ms,
            end_time_ms = spell_end_time_ms,
            interruptible = (not spell_uninterruptible),
            channeling = false,
        }
    elseif channel_name ~= nil then
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

-- Returns nil if the token (like "nameplateX") corresponding to the UnitGUID cannot be found any more.
local function NameplateInfoFromGUID(unit_guid)
    local token = UnitTokenFromGUID(unit_guid)
    if not token or UnitIsDeadOrGhost(token) then return nil end

    local helpful_auras = {}
    local harmful_auras = {}
    AuraUtil.ForEachAura(token, "HELPFUL", nil, function(aura) HandleAura(aura, helpful_auras) end, true)
    AuraUtil.ForEachAura(token, "HARMFUL", nil, function(aura) HandleAura(aura, harmful_auras) end, true)

    local value = {
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
        -- TODO: consider timers on unavoidable trash AOE spells?
    }
    return value
end


-- Some state information that gets updated when EVENTS fire:
local enemy_nameplate_info = {}
local combat_entry_timestamp_ms = nil
local combat_exit_timestamp_ms = nil

function game_state_frame:NAME_PLATE_UNIT_ADDED(unit_id)
    local key = UnitGUID(unit_id)
    if not key then return end
    enemy_nameplate_info[key] = NameplateInfoFromGUID(key)
end
function game_state_frame:NAME_PLATE_UNIT_REMOVED(unit_id)
    local key = UnitGUID(unit_id)
    if not key then return end
    enemy_nameplate_info[key] = nil
end
function game_state_frame:PLAYER_REGEN_DISABLED()
    combat_entry_timestamp_ms = mathfloor(1000*GetTime())
end
function game_state_frame:PLAYER_REGEN_ENABLED()
    combat_exit_timestamp_ms = mathfloor(1000*GetTime())
end

function game_state_frame:UI_ERROR_MESSAGE(message_type, message)
    -- Use this to check `facing` requirements.
    -- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Events/Player.lua#L240C1-L241C1
end

game_state_frame:RegisterEvent("NAME_PLATE_UNIT_ADDED")
game_state_frame:RegisterEvent("NAME_PLATE_UNIT_REMOVED")
game_state_frame:RegisterEvent("PLAYER_REGEN_DISABLED")
game_state_frame:RegisterEvent("PLAYER_REGEN_ENABLED")
game_state_frame:RegisterEvent("UI_ERROR_MESSAGE")
game_state_frame:SetScript("OnEvent", function(self, event, ...) self[event](self, ...) end)



-- ############# BUILD GAME SNAPSHOT FUNCS ##################

local function BuildResourcesTable()
end

local function BuildCooldownsTable()
end

local function BuildUnitBaseTable(unit_id)
end

local function GetEnemyFlags(unit_id)
    local v = 0
    local facing = false
end

local function BuildPlayerUnitTable()
    local player_table = {
        resources = BuildResourcesTable(),
        cooldowns = BuildCooldownsTable(),
        base = BuildUnitBaseTable("player"),
    }
    return player_table
end

local function BuildEnemyUnitTable(unit_id)
    local enemy_table = {
        guid_hash = 696969,
        range = 30,
        flags = GetEnemyFlags(unit_id),
        base = BuildUnitBaseTable(unit_id),
    }
    return enemy_table
end

local function BuildEnemyUnitsTable()
    local enemies_table = {}
    for unit_guid, info in ipairs(enemy_nameplate_info) do
        local enemy_unit = {
            guid_hash = 696969,
            range = 30,
            flags = 123,
            base = BuildUnitBaseTable(UnitTokenFromGUID(unit_guid)),
        }

        enemies_table.insert(BuildEnemyUnitTable(enemy_nameplate))
    end
end

local function BuildSnapshotTable()
    local snapshot_time_ms = mathfloor(1000 * GetTime()) -- this field is generally valuable and we only want to do one call to GetTime to compute it per update.

    local snapshot_table = {
        snapshot_time_ms = snapshot_time_ms,
        combat_time_ms = (combat_entry_timestamp_ms ~= nil) and snapshot_time_ms - combat_entry_timestamp_ms or 0,
        target_guid_hash = 1234567, -- faked/stubbed here until we have a hash function implemented on string GUIDs
        player = BuildPlayerUnitTable(),
        enemies = BuildEnemiesTable(),
    }
    return snapshot_table
end

function GameStateSnapshotter:GetSnapshot()
end

function GameStateSnapshotter:DoThing()
end


Portunus.Modules.GameStateSnapshotter = GameStateSnapshotter