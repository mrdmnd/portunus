local CombatTracker = {}

local onscreenNameplates = {}
local trackedEnemies = {}

CombatTracker.onscreenNameplates = onscreenNameplates
CombatTracker.trackedEnemies = trackedEnemies

function CombatTracker:Initialize()
    self:RegisterEvents()
end

-- Event Dispatcher
function CombatTracker:RegisterEvents()
    local frame = CreateFrame("Frame")
    frame:RegisterEvent("PLAYER_REGEN_DISABLED")
    frame:RegisterEvent("PLAYER_REGEN_ENABLED")
    frame:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
    frame:RegisterEvent("NAME_PLATE_UNIT_ADDED")
    frame:RegisterEvent("NAME_PLATE_UNIT_REMOVED")
    frame:SetScript("OnEvent", function(_, event, ...)
        if event == "COMBAT_LOG_EVENT_UNFILTERED" then
            self:HandleCombatLogEvent(CombatLogGetCurrentEventInfo())
        elseif event == "PLAYER_REGEN_DISABLED" then
            print("entering combat")
            self:OnEnterCombat()
        elseif event == "PLAYER_REGEN_ENABLED" then
            print("exiting combat")
            self:OnLeaveCombat()
        elseif event == "NAME_PLATE_UNIT_ADDED" then
            self:HandleNameplateAdded(...)
        elseif event == "NAME_PLATE_UNIT_REMOVED" then
            self:HandleNameplateRemoved(...)
        end
    end)
end

function CombatTracker:HandleCombatLogEvent(...)
    local _, subevent, _, sourceGUID, _, _, _, destGUID = ...
    if subevent == "SWING_DAMAGE" or subevent:match("_DAMAGE$") then
        if sourceGUID == UnitGUID("player") then
            self:UpdateEnemy(destGUID, true)
        elseif destGUID == UnitGUID("player") then
            self:UpdateEnemy(sourceGUID, true)
        end
    end
end

function CombatTracker:HandleNameplateAdded(unitID)
    local guid = UnitGUID(unitID)
    if guid and UnitIsEnemy("player", unitID) then -- CHECK: do we need this to also check the threat situation?
        self:UpdateEnemy(guid, false, true)
    end
end

function CombatTracker:HandleNameplateRemoved(unitID)
    local guid = UnitGUID(unitID)
    if guid then
        self:UpdateEnemy(guid, false, false)
    end
end

function CombatTracker:UpdateEnemy(guid, inCombat, inProximity)
    local enemy = trackedEnemies[guid]
    if not enemy then
        enemy = {inCombat = false, inProximity = false}
        trackedEnemies[guid] = enemy
    end
    local statusChanged = false
    if inCombat ~= nil and enemy.inCombat ~= inCombat then
        enemy.inCombat = inCombat
        statusChanged = true
    end
    if inProximity ~= nil and enemy.inProximity ~= inProximity then
        enemy.inProximity = inProximity
        statusChanged = true
    end
    if statusChanged then
        self:OnEnemyStatusChanged(guid, enemy)
    end
    if not enemy.inCombat and not enemy.inProximity then
        trackedEnemies[guid] = nil
        self:OnEnemyRemoved(guid)
    end
end

function CombatTracker:GetTrackedEnemies()
    return trackedEnemies
end

function CombatTracker:OnEnterCombat()
    -- Handle entering combat
end

function CombatTracker:OnLeaveCombat()
    -- Clear the enemies table when leaving combat
    for guid, enemy in pairs(trackedEnemies) do
        enemy.inCombat = false
        self:OnEnemyStatusChanged(guid, enemy)
    end
end

function CombatTracker:OnEnemyStatusChanged(guid, enemy)
    -- Handle when an enemy's status changes
    -- You can implement specific logic here based on the new status
end

function CombatTracker:OnEnemyRemoved(guid)
    -- Handle when an enemy is completely removed from tracking
end

-- Initialize the module
CombatTracker:Initialize()


Portunus.Modules.CombatTracker = CombatTracker