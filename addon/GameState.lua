local _, Portunus = ...

local Flatbuffers = Portunus.Modules["flatbuffers"]
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

local enemy_nameplate_info = {}
---@diagnostic disable-next-line: inject-field
function game_state_frame:NAME_PLATE_UNIT_ADDED(unit_id)
    local key = UnitGUID(unit_id)
    if not key then return end
    enemy_nameplate_info[key] = NameplateInfoFromGUID(key)
end
---@diagnostic disable-next-line: inject-field
function game_state_frame:NAME_PLATE_UNIT_REMOVED(unit_id)
    local key = UnitGUID(unit_id)
    if not key then return end
    enemy_nameplate_info[key] = nil
end
game_state_frame:RegisterEvent("NAME_PLATE_UNIT_ADDED")
game_state_frame:RegisterEvent("NAME_PLATE_UNIT_REMOVED")
game_state_frame:SetScript("OnEvent", function(self, event, ...) self[event](self, ...) end)


-- The "main" method for this module.

local function GetGameStateAsFlatbuffer()
    local builder = Flatbuffers.Builder(1024)

    GameState.Start(builder)
    GameState.AddSnapshotTimeMs(builder, mathfloor(1000*GetTime()))
    GameState.AddTargetGuid(builder, builder:CreateString(UnitGUID("target")))

    -- Get resource information
    -- This section should be class specific but we can test on spriest
    local resource_type = ResourceType.Insanity
    local current_resource = UnitPower("player", resource_type)
    local max_resource = UnitPowerMax("player", resource_type)
    Resource.Start(builder)
    Resource.AddResourceType(builder, resource_type)
    Resource.AddCurrent(builder, current_resource)
    Resource.AddMaximum(builder, max_resource)
    local resource = Resource.End(builder)

    -- Get cooldown information
    Cooldown.Start(builder)
    Cooldown.AddSpellId(builder, 12345) -- Example Spell id
    Cooldown.AddReady(builder, 1000*GetTime() + 12345) -- Example ready time
    Cooldown.AddCharges(builder, 3) -- Example charges
    local cooldown = Cooldown.End(builder)

    -- Get spell cast information
    SpellCastInfo.Start(builder)
    SpellCastInfo.AddSpellId(builder, 67890) -- example spell id
    SpellCastInfo.AddStartTimeMs(builder, 1000)
    SpellCastInfo.AddEndTimeMs(builder, 3000)
    SpellCastInfo.AddInterruptible(builder, true)
    SpellCastInfo.AddChanneling(builder, false)
    local spell_cast_info = SpellCastInfo.End(builder)

    -- Get aura information
    Aura.Start(builder)
    Aura.AddSpellId(builder, 12345)  -- example spell ID
    Aura.AddExpires(builder, 2000)  -- example expiration time
    Aura.AddStacks(builder, 2)  -- example stacks
    local buff = Aura.End(builder)

    Aura.Start(builder)
    Aura.AddSpellId(builder, 67890)  -- example spell ID
    Aura.AddExpires(builder, 3000)  -- example expiration time
    Aura.AddStacks(builder, 1)  -- example stacks
    local debuff = Aura.End(builder)


    -- Create UnitBase for player
    UnitBase.Start(builder)
    UnitBase.AddHealthCurrent(builder, 5000)  -- example current health
    UnitBase.AddHealthMaximum(builder, 10000)  -- example max health
    UnitBase.AddSpeed(builder, 1.0)  -- example speed
    UnitBase.AddSpellCastInfo(builder, spell_cast_info)
    UnitBase.StartBuffsVector(builder, 1)
    builder:PrependUOffsetTRelative(buff)
    local buffs = builder:EndVector(1)
    UnitBase.AddBuffs(builder, buffs)
    UnitBase.StartDebuffsVector(builder, 1)
    builder:PrependUOffsetTRelative(debuff)
    local debuffs = builder:EndVector(1)
    UnitBase.AddDebuffs(builder, debuffs)
    local unit_base = UnitBase.End(builder)

    -- Create PlayerUnit from UnitBase, cooldowns, and resources
    PlayerUnit.Start(builder)
    PlayerUnit.AddBase(builder, unit_base)
    PlayerUnit.StartCooldownsVector(builder, 1)
    builder:PrependUOffsetTRelative(cooldown)
    local cooldowns = builder:EndVector(1)
    PlayerUnit.AddCooldowns(builder, cooldowns)
    PlayerUnit.StartResourcesVector(builder, 1)
    builder:PrependUOffsetTRelative(resource)
    local resources = builder:EndVector(1)
    PlayerUnit.AddResources(builder, resources)
    local player_unit = PlayerUnit.End(builder)

    -- Add PlayerUnit to GameState
    GameState.AddPlayerUnit(builder, player_unit)


    -- Fill in enemy units
    GameState.StartVisibleTargetsVector(builder, #enemy_nameplate_info)
    for unit_guid, enemy_info in ipairs(enemy_nameplate_info) do
        -- Create enemy_unit_base core info. Could add more info as needed.
        UnitBase.Start(builder)
        --UnitBase.AddHealthCurrent(builder, enemy_info.health_current)
        UnitBase.AddHealthCurrent(builder, 100)
        --UnitBase.AddHealthMaximum(builder, enemy_info.health_maximum)
        UnitBase.AddHealthMaximum(builder, 200)
        --UnitBase.AddSpeed(builder, enemy_info.speed)
        UnitBase.AddSpeed(builder, 1.15)
        local enemy_unit_base = UnitBase.End(builder)

        -- Create enemy unit
        EnemyUnit.Start(builder)
        EnemyUnit.AddBase(builder, enemy_unit_base)
        EnemyUnit.AddUnitGuid(builder, builder:CreateString("guid goes here"))
        EnemyUnit.AddName(builder, builder:CreateString("samwise gamgee"))
        -- ...
        --EnemyUnit.AddNameplateVisible()

        local enemy_unit = EnemyUnit.End(builder)
        builder:PrependUOffsetTRelative(enemy_unit)
    end
    local visible_targets = builder:EndVector(#enemy_nameplate_info)
    GameState.AddVisibleTargets(visible_targets)

    -- Finalize gamestate and return the buffer.
    local game_state = GameState.End(builder)
    builder:Finish(game_state)
    return builder:Output()
end

Portunus.ExportGameState = GetGameStateAsFlatbuffer