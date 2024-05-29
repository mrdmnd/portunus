local _, Portunus = ...

Portunus.GameState = {}

local function dump(o)
   if type(o) == 'table' then
      local s = '{ '
      for k,v in pairs(o) do
         if type(k) ~= 'number' then k = '"'..k..'"' end
         s = s .. '['..k..'] = ' .. dump(v) .. ','
      end
      return s .. '} '
   else
      return tostring(o)
   end
end


-- Unit ID vs Unit GUID vs NPC ID thoughts?
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Class/Unit/Main.lua#L39

-- Handle training dummies appropriately.
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Class/Unit/Main.lua#L160

-- Determine if we are tanking the unit
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Class/Unit/Main.lua#L312

-- Determine if the unit is moving
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Class/Unit/Main.lua#L330

-- Determine our point-in-time power regeneration:
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Class/Unit/Power.lua#L46

-- Range checking is gonna be a Pretty Hard Problem to tackle.
-- https://github.com/herotc/hero-lib/blob/dragonflight/HeroLib/Class/Unit/Range.lua

-- It's not clear if we really need TTD tracking at all but maybe we do.
-- https://github.com/herotc/hero-lib/blob/dragonflight/HeroLib/Class/Unit/TimeToDie.lua

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


local game_state_frame = CreateFrame("Frame", "game_state_frame")
local enemy_nameplate_info = {}

-- TODO: the packed aura actually has lots of shit we don't care about; we should prune this down to minimize memory usage in our state table.
local function HandleAura(aura, output_table)
    output_table[#output_table+1] = {
        name = aura.name,
        spell_id = aura.spellId,
        source = aura.sourceUnit, -- Token of the unit that applied the aura.
        expires = aura.expirationTime, -- AuraRemains is just expirationTime - GetTime()
        stacks = aura.applications,
        --aura_instance_id = aura.auraInstanceID,
        --points = aura.points,
    }
end

-- Returns nil if the token (like "nameplateX") corresponding to the UnitGUID cannot be found any more.
local function NameplateInfoFromGUID(UnitGUID)
    local token = UnitTokenFromGUID(UnitGUID)
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
        cast_info = {UnitCastingInfo(token)},
        channel_info = {UnitChannelInfo(token)},
        helpful_auras = helpful_auras,
        harmful_auras = harmful_auras,
        -- TODO: range = ...
        -- TODO: consider timers on unavoidable trash AOE spells?
    }
    return value
end

---@diagnostic disable-next-line: inject-field
function game_state_frame:NAME_PLATE_UNIT_ADDED(UnitID)
    local key = UnitGUID(UnitID)
    if not key then return end
    enemy_nameplate_info[key] = NameplateInfoFromGUID(key)
end
---@diagnostic disable-next-line: inject-field
function game_state_frame:NAME_PLATE_UNIT_REMOVED(UnitID)
    local key = UnitGUID(UnitID)
    if not key then return end
    enemy_nameplate_info[key] = nil
end



game_state_frame:RegisterEvent("NAME_PLATE_UNIT_ADDED")
game_state_frame:RegisterEvent("NAME_PLATE_UNIT_REMOVED")
game_state_frame:SetScript("OnEvent", function(self, event, ...) self[event](self, ...) end)



-- Returns start time, duration
local function GCDInfo()
    return GetSpellCooldown(61304)
end

-- At a high level, we should put things that do "logic" into the rust UI code. For example, no unit blacklisting here, etc.

-- Implement a "player inspector" that fires once when we zone in or change static stuff, etc.
-- Don't put this call in the ReadFullGameState() function.
-- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Events/Player.lua#L133

-- The "main" method for this module.
local function ReadFullGameState()
    local snapshot_time = GetTime()
    local gs = {}

    gs.SnapshotTime = snapshot_time

    gs.Player = {}
    gs.Player.HelpfulAuras = {}
    gs.Player.HarmfulAuras = {}
    AuraUtil.ForEachAura("player", "HELPFUL", nil, function(aura) HandleAura(aura, gs.Player.HelpfulAuras) end, true)
    AuraUtil.ForEachAura("player", "HARMFUL", nil, function(aura) HandleAura(aura, gs.Player.HarmfulAuras) end, true)

    -- gs.Player.Cooldowns = {}

    -- Health, mana, insanity, combo points, etc.
    -- gs.Player.Resources = {}

    -- These may update with procs, etc.
    -- gs.Player.Stats = {}

    -- Probably also want a static player config section for talents, etc. That can go here.
    -- gs.Player.Persistent = {}

    -- SpellInFlight, CurrentTarget, stuff like that?, CombatStarted, CombatEnded, etc
    -- gs.Player.Misc = {}

    -- TODO: snapshot pmultiplier info for things like "empowered garrotes on a target" or whatever
    -- TODO: determine if the nameplate is "in front" of the character for spells that care about that
    -- https://github.com/herotc/hero-lib/blob/cf5f826ae1600a8bac75f82f9818e629a9a9213e/HeroLib/Events/Player.lua#L239

    -- Update the table of enemy units.
    for unit_guid, _ in pairs(enemy_nameplate_info) do
        enemy_nameplate_info[unit_guid] = NameplateInfoFromGUID(unit_guid)
    end
    gs.EnemyUnits = enemy_nameplate_info

    -- For healing / other stuff where you want to check on the state of your group members.
    -- gs.FriendlyUnits = {}


    -- local gcd_start, gcd_duration = GCDInfo()
    -- if gcd_start > 0 then
    --     gs.GCDInfo = {start = gcd_start, duration = gcd_duration, remains = gcd_start + gcd_duration - snapshot_time}
    -- else
    --     gs.GCDInfo = {start = 0, duration = 0, remains = 0}
    -- end

    return gs
end

Portunus.GameState.GetGameState = ReadFullGameState