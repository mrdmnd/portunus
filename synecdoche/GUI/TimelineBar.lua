--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- Lua
local pairs = pairs

--- ================ TIMELINE BAR ================
-- Timeline Bar - vertical bar with scrolling category icons (top -> bottom)
SYN.TimelineBarFrame = CreateFrame("Frame", "SYN_TimelineBarFrame", UIParent)
SYN.TimelineBarFrame.IconById = {}
SYN.TimelineBarFrame.ActiveIds = {}

-- Category -> icon texture mapping
local CATEGORY_ICONS = {
    movement = "Interface\\Icons\\Ability_Rogue_Sprint",
    defensive = "Interface\\Icons\\Ability_Warrior_ShieldWall",
    burst = "Interface\\Icons\\Ability_Warrior_Revenge",
    immune = "Interface\\Icons\\Spell_Holy_DivineProtection",
}

function SYN.TimelineBarFrame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    -- Vertical bar geometry
    self:SetWidth(18)
    self:SetHeight(220)
    -- Place to the left of all combat frames (anchor to main icon cluster)
    self:SetPoint("BOTTOMRIGHT", SYN.LeftIconFrame, "BOTTOMLEFT", -10, 0)

    -- Background and border
    self.Bg = self:CreateTexture(nil, "BACKGROUND")
    self.Bg:SetAllPoints(self)
    self.Bg:SetColorTexture(0, 0, 0, 0.5)
    SYN.CreateBackdrop(self)

    -- t=0 marker at the bottom
    self.ZeroMarker = self:CreateTexture(nil, "ARTWORK")
    self.ZeroMarker:SetColorTexture(1, 1, 1, 0.8)
    self.ZeroMarker:SetPoint("BOTTOM", self, "BOTTOM", 0, 0)
    self.ZeroMarker:SetHeight(2)
    self.ZeroMarker:SetWidth(self:GetWidth())

    -- seconds spanned by the bar from bottom (t=0) to top (t=horizon)
    self.HorizonSeconds = 15

    self:Show()
end

function SYN.TimelineBarFrame:GetIconForEvent(ev)
    local id = ev.id
    local icon = self.IconById[id]
    if icon then return icon end

    icon = CreateFrame("Frame", nil, self)
    icon:SetSize(18, 18)
    icon.Tex = icon:CreateTexture(nil, "ARTWORK")
    icon.Tex:SetAllPoints(icon)
    local tex = CATEGORY_ICONS[ev.category or ""]
    if not tex then
        tex = "Interface\\Icons\\INV_Misc_QuestionMark"
    end
    icon.Tex:SetTexture(tex)
    SYN.CreateBackdrop(icon)
    -- Label shown to the left of the icon
    local label = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    icon.Label = label
    label:SetJustifyH("RIGHT")
    label:SetJustifyV("MIDDLE")
    label:SetTextColor(1, 1, 1, 1)
    label:SetFont(SYN.FontSelect(label), 10, "OUTLINE")
    label:SetText(ev.label or "")
    label:Show()
    -- Duration line drawn to the right of the timeline bar
    -- Parent to the bar so it can sit just outside the bar's right edge
    local dline = self:CreateTexture(nil, "ARTWORK")
    icon.DurationLine = dline
    dline:SetColorTexture(1, 1, 1, 0.9)
    dline:SetWidth(2)
    dline:Hide()
    icon:Show()

    self.IconById[id] = icon
    return icon
end

function SYN.TimelineBarFrame:UpdateIcons()
    if not SYN.Timeline or not SYN.Timeline.All then
        -- Hide the bar visuals if Timeline isn't initialized yet
        if self.Bg and self.Bg:IsShown() then
            self.Bg:Hide()
            if self.Backdrop then self.Backdrop:Hide() end
            if self.ZeroMarker then self.ZeroMarker:Hide() end
        end
        return
    end
    
    local events = SYN.Timeline:All()
    
    -- Safety check
    if not events then
        events = {}
    end

    -- Mark all as inactive initially
    for k in pairs(self.ActiveIds) do self.ActiveIds[k] = nil end

    local now = GetTime()
    local height = self:GetHeight()
    local horizon = self.HorizonSeconds
    local hasVisibleEvents = false

    for i = 1, #events do
        local ev = events[i]
        local inSec = (ev.startAt or now) - now
        local expiresAt = ev.expiresAt or now
        
        -- Show event until its endpoint crosses zero
        if inSec <= horizon and expiresAt >= now then
            hasVisibleEvents = true
            local icon = self:GetIconForEvent(ev)
            -- Position: top edge is horizon, bottom edge is t=0
            local y = (1 - (inSec / horizon)) * height
            icon:ClearAllPoints()
            -- Place so the icon's bottom edge is at the time position
            icon:SetPoint("BOTTOM", self, "TOP", 0, -y)
            -- Update label text and position to the left of the icon
            if icon.Label then
                icon.Label:ClearAllPoints()
                icon.Label:SetText(ev.label or "")
                icon.Label:SetPoint("RIGHT", icon, "LEFT", -4, 0)
                icon.Label:Show()
            end
            -- Draw duration line for events with a duration
            local dur = ev.duration or 0
            if icon.DurationLine then
                if dur and dur > 0 then
                    -- Set color to green if event is live (started), white if upcoming
                    if inSec <= 0 then
                        icon.DurationLine:SetColorTexture(0, 1, 0, 0.9)  -- Green when live
                    else
                        icon.DurationLine:SetColorTexture(1, 1, 1, 0.9)  -- White when upcoming
                    end
                    local endTime = (ev.expiresAt or (ev.startAt or now))
                    local maxTime = now + horizon
                    if endTime > maxTime then endTime = maxTime end
                    local durPixels = math.max(0, (endTime - (ev.startAt or now)) / horizon * height)
                    -- Start at icon's bottom-right corner, extend upward (toward horizon)
                    local bottomOfIconOffsetFromTop = y
                    -- Anchor the line's top and bottom to the bar's TOPRIGHT so we can position by offsets
                    icon.DurationLine:ClearAllPoints()
                    -- Bottom of the line at the icon's bottom
                    icon.DurationLine:SetPoint("BOTTOMRIGHT", self, "TOPRIGHT", 2, -bottomOfIconOffsetFromTop)
                    -- Top of the line durPixels above the icon (toward horizon)
                    icon.DurationLine:SetPoint("TOPRIGHT", self, "TOPRIGHT", 2, -(bottomOfIconOffsetFromTop - durPixels))
                    icon.DurationLine:Show()
                else
                    icon.DurationLine:Hide()
                end
            end
            icon:Show()
            self.ActiveIds[ev.id] = true
        end
    end

    -- Hide icons no longer active
    for id, icon in pairs(self.IconById) do
        if not self.ActiveIds[id] then
            icon:Hide()
            if icon.Label then icon.Label:Hide() end
            if icon.DurationLine then icon.DurationLine:Hide() end
        end
    end

    -- Hide or show the timeline bar CHILDREN based on whether there are visible events
    -- Note: We can't hide the frame itself because OnUpdate won't fire if the frame is hidden!
    -- Instead, hide/show the background, border, and zero marker
    if hasVisibleEvents then
        -- Show background, border, and zero marker
        if self.Bg and not self.Bg:IsShown() then
            self.Bg:Show()
            if self.Backdrop then self.Backdrop:Show() end
            if self.ZeroMarker then self.ZeroMarker:Show() end
        end
    else
        -- Hide background, border, and zero marker
        if self.Bg and self.Bg:IsShown() then
            self.Bg:Hide()
            if self.Backdrop then self.Backdrop:Hide() end
            if self.ZeroMarker then self.ZeroMarker:Hide() end
        end
    end
end

