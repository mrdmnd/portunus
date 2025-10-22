--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- Lua
local pairs = pairs
local ipairs = ipairs
local tostring = tostring
local math = math
-- WoW API
local GetTime = GetTime
local UnitExists = UnitExists
local UnitGUID = UnitGUID

--- ================= ENEMY TRACKER =================
-- Displays all enemies in combat with the party as health bars
-- on the right edge of the main addon frame
SYN.EnemyTrackerFrame = CreateFrame("Frame", "SYN_EnemyTrackerFrame", UIParent)
SYN.EnemyTrackerFrame.HealthBars = {}
SYN.EnemyTrackerFrame.ActiveBars = {}

-- Create a health bar for an enemy
local function CreateEnemyHealthBar(parent)
    local bar = CreateFrame("Frame", nil, parent)
    bar:SetSize(120, 18)
    bar:SetFrameStrata(parent:GetFrameStrata())
    bar:SetFrameLevel(parent:GetFrameLevel() + 1)
    
    -- Background
    bar.Bg = bar:CreateTexture(nil, "BACKGROUND")
    bar.Bg:SetAllPoints(bar)
    bar.Bg:SetColorTexture(0.1, 0.1, 0.1, 0.8)
    
    -- Health bar texture
    bar.Health = bar:CreateTexture(nil, "ARTWORK")
    bar.Health:SetPoint("LEFT", bar, "LEFT", 0, 0)
    bar.Health:SetHeight(18)
    bar.Health:SetColorTexture(0.0, 0.8, 0.0, 0.9)
    
    -- Border/backdrop
    SYN.CreateBackdrop(bar)
    bar.Backdrop:SetBackdropBorderColor(0.3, 0.3, 0.3, 1.0)
    
    -- Name text
    bar.NameText = bar:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    bar.NameText:SetPoint("LEFT", bar, "LEFT", 3, 0)
    bar.NameText:SetJustifyH("LEFT")
    bar.NameText:SetTextColor(1, 1, 1, 1)
    local font = SYN.FontSelect(bar.NameText)
    bar.NameText:SetFont(font, 10, "OUTLINE")
    
    -- Health text
    bar.HealthText = bar:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    bar.HealthText:SetPoint("RIGHT", bar, "RIGHT", -3, 0)
    bar.HealthText:SetJustifyH("RIGHT")
    bar.HealthText:SetTextColor(1, 1, 1, 1)
    bar.HealthText:SetFont(font, 10, "OUTLINE")
    
    -- Highlight frame for target/mouseover (initially hidden)
    bar.Highlight = CreateFrame("Frame", nil, bar)
    bar.Highlight:SetAllPoints(bar)
    bar.Highlight:SetFrameLevel(bar:GetFrameLevel() + 2)
    SYN.CreateBackdrop(bar.Highlight)
    bar.Highlight.Backdrop:SetBackdropBorderColor(1, 1, 1, 1.0)
    bar.Highlight.Backdrop:SetBackdropColor(0, 0, 0, 0) -- Transparent center
    bar.Highlight:Hide()
    
    -- Cast bar (initially hidden)
    bar.CastBar = CreateFrame("Frame", nil, bar)
    bar.CastBar:SetSize(120, 18)
    bar.CastBar:SetPoint("LEFT", bar, "RIGHT", 2, 0)
    bar.CastBar:SetFrameLevel(bar:GetFrameLevel())
    
    -- Cast bar background
    bar.CastBar.Bg = bar.CastBar:CreateTexture(nil, "BACKGROUND")
    bar.CastBar.Bg:SetAllPoints(bar.CastBar)
    bar.CastBar.Bg:SetColorTexture(0.1, 0.1, 0.1, 0.8)
    
    -- Cast bar progress
    bar.CastBar.Progress = bar.CastBar:CreateTexture(nil, "ARTWORK")
    bar.CastBar.Progress:SetPoint("LEFT", bar.CastBar, "LEFT", 0, 0)
    bar.CastBar.Progress:SetHeight(12)
    bar.CastBar.Progress:SetColorTexture(1.0, 0.7, 0.0, 0.9)
    
    -- Cast bar border
    SYN.CreateBackdrop(bar.CastBar)
    bar.CastBar.Backdrop:SetBackdropBorderColor(0.3, 0.3, 0.3, 1.0)
    
    -- Cast bar text (spell name)
    bar.CastBar.Text = bar.CastBar:CreateFontString(nil, "OVERLAY", 
                                                     "GameFontHighlight")
    bar.CastBar.Text:SetPoint("LEFT", bar.CastBar, "LEFT", 3, 0)
    bar.CastBar.Text:SetJustifyH("LEFT")
    bar.CastBar.Text:SetTextColor(1, 1, 1, 1)
    bar.CastBar.Text:SetFont(font, 9, "OUTLINE")
    
    -- Cast bar time remaining text
    bar.CastBar.TimeText = bar.CastBar:CreateFontString(nil, "OVERLAY", 
                                                        "GameFontHighlight")
    bar.CastBar.TimeText:SetPoint("RIGHT", bar.CastBar, "RIGHT", -3, 0)
    bar.CastBar.TimeText:SetJustifyH("RIGHT")
    bar.CastBar.TimeText:SetTextColor(1, 1, 1, 1)
    bar.CastBar.TimeText:SetFont(font, 9, "OUTLINE")
    
    bar.CastBar:Hide()
    
    bar.guid = nil
    bar:Hide()
    return bar
end

function SYN.EnemyTrackerFrame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(120)
    self:SetHeight(300)
    
    -- Position to the right of the main frame cluster
    self:SetPoint("BOTTOMLEFT", SYN.Prediction2Frame, "BOTTOMRIGHT", 10, 0)
    
    -- Background (only shows when there are bars)
    self.Bg = self:CreateTexture(nil, "BACKGROUND")
    self.Bg:SetAllPoints(self)
    self.Bg:SetColorTexture(0, 0, 0, 0.3)
    self.Bg:Hide()
    
    -- Create backdrop
    SYN.CreateBackdrop(self)
    self.Backdrop:Hide()
    
    self:Show()
end

function SYN.EnemyTrackerFrame:GetHealthBar()
    -- Find an unused bar
    for i, bar in ipairs(self.HealthBars) do
        if not bar.guid then
            return bar
        end
    end
    
    -- Create a new bar if none available
    local bar = CreateEnemyHealthBar(self)
    table.insert(self.HealthBars, bar)
    return bar
end

function SYN.EnemyTrackerFrame:UpdateBars()
    -- Check if Battlefield is initialized
    if not SYN.Battlefield or not SYN.Battlefield.GetCombatUnits then
        if self.Bg:IsShown() then
            self.Bg:Hide()
            self.Backdrop:Hide()
        end
        return
    end
    
    local combatUnits = SYN.Battlefield:GetCombatUnits()
    if not combatUnits then
        if self.Bg:IsShown() then
            self.Bg:Hide()
            self.Backdrop:Hide()
        end
        return
    end
    
    -- Clear active tracking and reset all bar guids for reuse
    for k in pairs(self.ActiveBars) do
        self.ActiveBars[k] = nil
    end
    for i, bar in ipairs(self.HealthBars) do
        bar.guid = nil
        bar:Hide()
        if bar.CastBar then
            bar.CastBar:Hide()
        end
    end
    
    -- Build sorted list of combat units
    local sortedUnits = {}
    for guid, unitData in pairs(combatUnits) do
        if not unitData.isDead and unitData.health > 0 then
            table.insert(sortedUnits, {
                guid = guid,
                name = unitData.name,
                health = unitData.health,
                healthMax = unitData.healthMax,
                castInfo = unitData.castInfo,
            })
        end
    end
    
    -- Sort by current health (absolute) ascending
    table.sort(sortedUnits, function(a, b)
        return a.health < b.health
    end)
    
    -- Check if we have any units to display
    local hasUnits = #sortedUnits > 0
    
    if hasUnits then
        -- Show background
        if not self.Bg:IsShown() then
            self.Bg:Show()
            self.Backdrop:Show()
        end
        
        -- Get current target and mouseover GUIDs
        local targetGUID = UnitExists("target") and UnitGUID("target") or nil
        local mouseoverGUID = UnitExists("mouseover") and UnitGUID("mouseover") or nil
        
        -- Position bars starting from bottom
        local barHeight = 18
        local barSpacing = 2
        local yOffset = 5 -- Start offset from bottom
        
        for i, unitInfo in ipairs(sortedUnits) do
            local bar = self:GetHealthBar()
            bar.guid = unitInfo.guid
            self.ActiveBars[unitInfo.guid] = true
            
            -- Update health bar
            local healthPct = unitInfo.health / unitInfo.healthMax
            bar.Health:SetWidth(self:GetWidth() * healthPct)
            
            -- Color based on health percentage
            if healthPct > 0.6 then
                bar.Health:SetColorTexture(0.0, 0.8, 0.0, 0.9) -- Green
            elseif healthPct > 0.3 then
                bar.Health:SetColorTexture(0.9, 0.9, 0.0, 0.9) -- Yellow
            else
                bar.Health:SetColorTexture(0.9, 0.0, 0.0, 0.9) -- Red
            end
            
            -- Update name (truncate if too long)
            local displayName = unitInfo.name
            if #displayName > 12 then
                displayName = displayName:sub(1, 10) .. ".."
            end
            bar.NameText:SetText(displayName)
            
            -- Update health text (show as K for thousands)
            local healthDisplay = unitInfo.health
            if healthDisplay >= 1000000 then
                healthDisplay = string.format("%.1fM", healthDisplay / 1000000)
            elseif healthDisplay >= 1000 then
                healthDisplay = string.format("%.0fk", healthDisplay / 1000)
            else
                healthDisplay = tostring(healthDisplay)
            end
            bar.HealthText:SetText(healthDisplay)
            
            -- Position from bottom upwards
            bar:ClearAllPoints()
            bar:SetPoint("BOTTOM", self, "BOTTOM", 0, yOffset)
            yOffset = yOffset + barHeight + barSpacing
            
            -- Update highlight for target/mouseover
            if unitInfo.guid == targetGUID then
                -- Green border for target
                bar.Highlight.Backdrop:SetBackdropBorderColor(0.0, 1.0, 0.0, 1.0)
                bar.Highlight:Show()
            elseif unitInfo.guid == mouseoverGUID then
                -- Yellow border for mouseover
                bar.Highlight.Backdrop:SetBackdropBorderColor(1.0, 1.0, 0.0, 1.0)
                bar.Highlight:Show()
            else
                bar.Highlight:Hide()
            end
            
            -- Update cast bar if unit is casting
            if unitInfo.castInfo then
                local castInfo = unitInfo.castInfo
                local currentTime = GetTime()
                local remaining = castInfo.endTime - currentTime
                local duration = castInfo.endTime - castInfo.startTime
                
                -- Calculate progress
                local progress = 0
                if castInfo.isChanneled then
                    -- For channels, progress goes from full to empty
                    progress = remaining / duration
                else
                    -- For casts, progress goes from empty to full
                    progress = 1 - (remaining / duration)
                end
                progress = math.max(0, math.min(1, progress))
                
                -- Update cast bar progress
                bar.CastBar.Progress:SetWidth(bar.CastBar:GetWidth() * progress)
                
                -- Color based on interruptible status
                if castInfo.notInterruptible then
                    -- Gray/silver for non-interruptible
                    bar.CastBar.Progress:SetColorTexture(0.5, 0.5, 0.5, 0.9)
                    bar.CastBar.Backdrop:SetBackdropBorderColor(0.5, 0.5, 0.5, 1.0)
                else
                    -- Orange for interruptible
                    bar.CastBar.Progress:SetColorTexture(1.0, 0.7, 0.0, 0.9)
                    bar.CastBar.Backdrop:SetBackdropBorderColor(1.0, 0.7, 0.0, 1.0)
                end
                
                -- Update spell name (truncate if needed)
                local spellName = castInfo.spellName or "Unknown"
                if #spellName > 14 then
                    spellName = spellName:sub(1, 12) .. ".."
                end
                bar.CastBar.Text:SetText(spellName)
                
                -- Update time remaining
                if remaining > 0 then
                    bar.CastBar.TimeText:SetText(string.format("%.1f", remaining))
                else
                    bar.CastBar.TimeText:SetText("0.0")
                end
                
                bar.CastBar:Show()
            else
                bar.CastBar:Hide()
            end
            
            bar:Show()
        end
    else
        -- Hide background
        if self.Bg:IsShown() then
            self.Bg:Hide()
            self.Backdrop:Hide()
        end
    end
end

function SYN.EnemyTrackerFrame:Reset()
    for i, bar in ipairs(self.HealthBars) do
        bar:Hide()
        bar.guid = nil
        if bar.CastBar then
            bar.CastBar:Hide()
        end
    end
    for k in pairs(self.ActiveBars) do
        self.ActiveBars[k] = nil
    end
    if self.Bg then
        self.Bg:Hide()
    end
    if self.Backdrop then
        self.Backdrop:Hide()
    end
end

