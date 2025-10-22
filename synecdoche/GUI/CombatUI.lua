--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- Synecdoche
local Utils = SYN.Utils
-- Lua
local pairs = pairs
local tostring = tostring
local stringlower = string.lower
local mathfloor = math.floor
-- File Locals



--- ================ CONTENTS ================
--- Main Frame
--                     ┌─────────┐                                              
--                     │         │                                              
--                     │    E    │                                              
--                     │         │                                              
--                     │         │                                              
--                     └─────────┘                                              
--               ┌────┐           ┌────┐                                        
--               │    │           │    │                                        
--               │  F │           │  G │                                        
--               └────┼───────────┼────┘    ┌───────────┐       ┌───────────┐   
--   ┌────────┐       │           │         │           │       │           │   
--   │        │       │           │         │           │       │           │   
--   │        │       │           │         │           │       │           │   
--   │    D   │       │     A     │         │     B     │       │     C     │   
--   │        │       │           │         │           │       │           │   
--   └────────┘       │           │         │           │       │           │   
--               ┌────┼───────────┼────┐    └───────────┘       └───────────┘   
--               │    │           │    │                                        
--               │ H  │           │  I │                                        
--               └────┘           └────┘                                        

--- ================ BASE ICON CLASS ================
-- Base class for all icon frames to eliminate code duplication
local function CreateBaseIconFrame(name, parent)
    local frame = CreateFrame("Frame", name, parent)
    
    -- Common methods that all icon frames will share
    function frame:SetCooldown(CooldownStart, CooldownDuration)
        if CooldownStart == 0 or CooldownDuration == 0 then
            self.CooldownFrame:SetCooldown(0, 0)
            self.CooldownFrame:Hide()
            return
        end
        self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
    end
    
    function frame:GetIconID()
        if self.ID then
            return self.ID
        end
        return nil
    end
    
    function frame:HideIcon()
        self:Hide()
        self.Backdrop:Hide()
        ActionButton_HideOverlayGlow(self)
    end
    
    function frame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
        -- If no ID or Texture is provided, hide the icon
        if not ID or not Texture then
            self:HideIcon()
            return
        end
        
        self.ID = ID
        self.Texture:SetTexture(Texture)
        if Unusable then
            self.Texture:SetVertexColor(0.5, 0.5, 1.0)
        elseif OutOfRange then
            self.Texture:SetVertexColor(1.0, 0.5, 0.5)
        else
            self.Texture:SetVertexColor(1.0, 1.0, 1.0)
        end
        self.Texture:SetAllPoints(self)
        
        -- Handle glow effect using WoW's built-in system
        if Glow then
            ActionButton_ShowOverlayGlow(self)
        else
            ActionButton_HideOverlayGlow(self)
        end
        
        local SelectedFont = SYN.FontSelect(self.Keybind)
        
        -- Use the frame's specific font sizes
        self.Keybind:SetFont(SelectedFont, self.KeybindFontSize or 14, "OUTLINE")
        self.Annotation:SetFont(SelectedFont, self.AnnotationFontSize or 8, "OUTLINE")

        if Keybind then
            self.Keybind:SetText(Keybind)
        else
            self.Keybind:SetText("")
        end

        if Annotation then
            self.Annotation:SetText(Annotation)
        else
            self.Annotation:SetText("")
        end

        if not self.Backdrop:IsVisible() then
            self.Backdrop:Show()
        end

        self:SetAlpha(1.0)
        if not self:IsVisible() then
            self:Show()
        end
    end
    
    return frame
end

-- Shared initialization function for all icon frames
local function InitializeIconFrame(frame, config)
    -- Frame setup
    frame:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    frame:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    frame:SetWidth(config.width)
    frame:SetHeight(config.height)
    frame:SetPoint(config.point, config.relativeTo, config.relativePoint, config.xOffset, config.yOffset)
    
    -- Store font sizes for ChangeIcon method
    frame.KeybindFontSize = config.keybindFontSize
    frame.AnnotationFontSize = config.annotationFontSize
    
    -- Texture
    frame.Texture = frame:CreateTexture(nil, "ARTWORK")
    frame.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    frame.Texture:SetAllPoints(frame)
    
    -- Cooldown
    frame.CooldownFrame = CreateFrame("Cooldown", nil, frame, "AR_CooldownFrameTemplate")
    frame.CooldownFrame:SetAllPoints(frame)
    
    -- Keybind
    local KeybindFrame = frame:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    frame.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), config.keybindFontSize, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = frame:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    frame.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), config.annotationFontSize, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    frame.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(frame)
    
    if config.showOnInit then
        frame:Show()
    else
        frame:Hide()
    end
end

-- A -- this is the main suggestion for your current target
SYN.MainIconFrame = CreateBaseIconFrame("SYN_MainIconFrame", UIParent)
SYN.MainIconFrame.CooldownFrame = CreateFrame("Cooldown", "SYN_MainIconCooldownFrame", SYN.MainIconFrame, "AR_CooldownFrameTemplate")

-- B -- this is the prediction "one timestep out" if you cast the thing in A or E (it presumes you choose E if E is rendered)
SYN.Prediction1Frame = CreateBaseIconFrame("SYN_Prediction1Frame", UIParent)

-- C
SYN.Prediction2Frame = CreateBaseIconFrame("SYN_Prediction2Frame", UIParent)

-- D -- this is the "what you should be casting if you weren't moving" icon; alternatively, this is what you'd switch to cast if you finished moving right this moment
SYN.LeftIconFrame = CreateBaseIconFrame("SYN_LeftIconFrame", UIParent)

-- E -- this shows up if you should be casting something on your non-current target. the main icon will fade slightly red if this shows up
SYN.TopIconFrame = CreateBaseIconFrame("SYN_TopIconFrame", UIParent)

-- F, G, H, I
-- Mostly allocate these for offensive CD (F), defensive CDs (G), off-GCD that shoudl be hit before main icon (H), and off-gcd that should be hit after main icon (I)
SYN.SmallTopLeftFrame = CreateBaseIconFrame("SYN_SmallTopLeftFrame", UIParent)
SYN.SmallTopRightFrame = CreateBaseIconFrame("SYN_SmallTopRightFrame", UIParent)
SYN.SmallBottomLeftFrame = CreateBaseIconFrame("SYN_SmallBottomLeftFrame", UIParent)
SYN.SmallBottomRightFrame = CreateBaseIconFrame("SYN_SmallBottomRightFrame", UIParent)

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

    -- OnUpdate drives icon positions
    self:SetScript("OnUpdate", function(_, elapsed)
        self:UpdateIcons()
    end)

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
    if not SYN.Timeline or not SYN.Timeline.All then return end
    local events = SYN.Timeline:All()

    -- Mark all as inactive initially
    for k in pairs(self.ActiveIds) do self.ActiveIds[k] = nil end

    local now = GetTime()
    local height = self:GetHeight()
    local horizon = self.HorizonSeconds

    for i = 1, #events do
        local ev = events[i]
        local inSec = (ev.startAt or now) - now
        -- Show event until its endpoint crosses zero
        if inSec <= horizon and (ev.expiresAt or now) >= now then
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
end

function SYN.ResetIcons()
    -- A - Main icon uses regular Hide() since it doesn't have HideIcon method
    SYN.MainIconFrame:Hide()
    SYN.MainIconFrame.Backdrop:Hide()
    
    -- All other frames use the inherited HideIcon method
    SYN.Prediction1Frame:HideIcon()
    SYN.Prediction2Frame:HideIcon()
    SYN.LeftIconFrame:HideIcon()
    SYN.TopIconFrame:HideIcon()
    SYN.SmallTopLeftFrame:HideIcon()
    SYN.SmallTopRightFrame:HideIcon()
    SYN.SmallBottomLeftFrame:HideIcon()
    SYN.SmallBottomRightFrame:HideIcon()
    -- Timeline bar icons are managed by UpdateIcons; just hide frame
    SYN.TimelineBarFrame:Hide()
end

function SYN.CreateBackdrop(Frame, Strata)
    if Frame.Backdrop then return; end
    local Backdrop = CreateFrame("Frame", nil, Frame, BackdropTemplateMixin and "BackdropTemplate")
    Frame.Backdrop = Backdrop
    Backdrop:ClearAllPoints()
    Backdrop:SetPoint("TOPLEFT", Frame, "TOPLEFT", -1, 1)
    Backdrop:SetPoint("BOTTOMRIGHT", Frame, "BOTTOMRIGHT", 1, -1)

    Backdrop:SetBackdrop({
        bgFile = "Interface\\ChatFrame\\ChatFrameBackground",
        edgeFile = "Interface\\ChatFrame\\ChatFrameBackground",
        tile = false,
        tileSize = 0,
        edgeSize = 1,
        insets = { left = 0, right = 0, top = 0, bottom = 0 }
    })

    Backdrop:SetBackdropBorderColor(0, 0, 1)
    Backdrop:SetBackdropColor(0, 0, 0, 1)
    if not Strata then Strata = SYN.MainFrame:GetFrameStrata(); end
    Backdrop:SetFrameStrata(Strata)
    if Frame:GetFrameLevel() >= 2 then
        Backdrop:SetFrameLevel(Frame:GetFrameLevel() - 2)
    else
        Backdrop:SetFrameLevel(0)
    end
end

function SYN.FontSelect(Frame)
    return GameFontNormal:GetFont()
end



--- ======= MAIN ICONS =======
function SYN.MainIconFrame:Init()
    InitializeIconFrame(self, {
        width = 64,
        height = 64,
        point = "CENTER",
        relativeTo = SYN.MainFrame,
        relativePoint = "CENTER",
        xOffset = 0,
        yOffset = 0,
        keybindFontSize = 14,
        annotationFontSize = 8,
        showOnInit = true
    })
    -- Set default keybind text for main icon
    self.Keybind:SetText("=)")
    self.Annotation:SetText("Placeholder")
end

-- This is the main function to set up the icon.
-- Texture: this is the main icon image.
-- Keybind: this is the reminder text for the keybind
-- Unusable: if true, we make the icon slightly blue-ish (shows up if you're out of mana or energy or whatever)
-- OutOfRange: if true, we make the icon slightly reddish (shows up when you're out of range)
-- ID: this is the spell id?
-- Annotation: this is the annotation text that shows up sometimes if you want the engine to pass notes to you
-- FontSize: this is the font size for the annotation
-- CooldownStart: this is the 
-- Glow: if true, adds a glow effect around the icon
-- Note: ChangeIcon, SetCooldown, and GetIconID methods are now inherited from the base class

--- ================ FRAME INITIALIZATION ================

-- B - Prediction1Frame (right of main icon)
function SYN.Prediction1Frame:Init()
    InitializeIconFrame(self, {
        width = 64,
        height = 64,
        point = "LEFT",
        relativeTo = SYN.MainIconFrame,
        relativePoint = "RIGHT",
        xOffset = 5,
        yOffset = 0,
        keybindFontSize = 12,
        annotationFontSize = 7,
        showOnInit = false
    })
end

-- C - Prediction2Frame (right of prediction1)
function SYN.Prediction2Frame:Init()
    InitializeIconFrame(self, {
        width = 64,
        height = 64,
        point = "LEFT",
        relativeTo = SYN.Prediction1Frame,
        relativePoint = "RIGHT",
        xOffset = 5,
        yOffset = 0,
        keybindFontSize = 12,
        annotationFontSize = 7,
        showOnInit = false
    })
end

-- D - LeftIconFrame (left of main icon)
function SYN.LeftIconFrame:Init()
    InitializeIconFrame(self, {
        width = 48,
        height = 48,
        point = "RIGHT",
        relativeTo = SYN.MainIconFrame,
        relativePoint = "LEFT",
        xOffset = -5,
        yOffset = 0,
        keybindFontSize = 12,
        annotationFontSize = 7,
        showOnInit = false
    })
end

-- E - TopIconFrame (above main icon)
function SYN.TopIconFrame:Init()
    InitializeIconFrame(self, {
        width = 48,
        height = 48,
        point = "BOTTOM",
        relativeTo = SYN.MainIconFrame,
        relativePoint = "TOP",
        xOffset = 0,
        yOffset = 5,
        keybindFontSize = 12,
        annotationFontSize = 7,
        showOnInit = false
    })
end

-- F - SmallTopLeftFrame (top-left corner of main icon)
function SYN.SmallTopLeftFrame:Init()
    InitializeIconFrame(self, {
        width = 32,
        height = 32,
        point = "BOTTOMRIGHT",
        relativeTo = SYN.MainIconFrame,
        relativePoint = "TOPLEFT",
        xOffset = 0,
        yOffset = 5,
        keybindFontSize = 10,
        annotationFontSize = 6,
        showOnInit = false
    })
end

-- G - SmallTopRightFrame (top-right corner of main icon)
function SYN.SmallTopRightFrame:Init()
    InitializeIconFrame(self, {
        width = 32,
        height = 32,
        point = "BOTTOMLEFT",
        relativeTo = SYN.MainIconFrame,
        relativePoint = "TOPRIGHT",
        xOffset = 0,
        yOffset = 5,
        keybindFontSize = 10,
        annotationFontSize = 6,
        showOnInit = false
    })
end

-- H - SmallBottomLeftFrame (bottom-left corner of main icon)
function SYN.SmallBottomLeftFrame:Init()
    InitializeIconFrame(self, {
        width = 32,
        height = 32,
        point = "TOPRIGHT",
        relativeTo = SYN.MainIconFrame,
        relativePoint = "BOTTOMLEFT",
        xOffset = 0,
        yOffset = -5,
        keybindFontSize = 10,
        annotationFontSize = 6,
        showOnInit = false
    })
end

-- I - SmallBottomRightFrame (bottom-right corner of main icon)
function SYN.SmallBottomRightFrame:Init()
    InitializeIconFrame(self, {
        width = 32,
        height = 32,
        point = "TOPLEFT",
        relativeTo = SYN.MainIconFrame,
        relativePoint = "BOTTOMRIGHT",
        xOffset = 0,
        yOffset = -5,
        keybindFontSize = 10,
        annotationFontSize = 6,
        showOnInit = false
    })
end



--- ================ CHANGE ICON METHODS ================
-- Note: All ChangeIcon methods are now inherited from the base class

--- ================ COOLDOWN AND ID METHODS ================
-- Note: All SetCooldown and GetIconID methods are now inherited from the base class

--- ================ HIDE ICON METHODS ================
-- Note: All HideIcon methods are now inherited from the base class

--- ================ fuckin' everything else lol ================


--- ================ NAMEPLATES ==================
SYN.Nameplates = {
    Frames = {},
    FramesByUnit = {},  -- Maps unit token -> frame
    ActiveUnits = {},   -- Set of currently active unit tokens
    UpdateFrame = nil,
    MouseoverUnit = nil,
    EnabledUnits = {},  -- Maps unit token -> boolean (for filtering)
    ShowAll = true,     -- If true, show frames for all enemies; if false, only show EnabledUnits
}

-- Create a frame for a nameplate
function SYN.Nameplates:CreateFrame()
    local frame = CreateFrame("Frame", nil, UIParent)
    frame:SetSize(40, 40)
    frame:SetFrameStrata("HIGH")
    frame:SetFrameLevel(100)
    
    -- Create the main texture
    frame.Texture = frame:CreateTexture(nil, "ARTWORK")
    frame.Texture:SetAllPoints(frame)
    frame.Texture:SetColorTexture(0.2, 0.6, 1.0, 0.3)
    
    -- Create backdrop/border
    SYN.CreateBackdrop(frame)
    frame.Backdrop:SetBackdropBorderColor(0.2, 0.6, 1.0, 0.5)
    
    frame.unit = nil  -- Track which unit this frame is attached to
    frame:Hide()
    return frame
end

-- Get or create a frame from the pool
function SYN.Nameplates:GetFrame(unit)
    -- Check if we already have a frame for this unit
    if self.FramesByUnit[unit] then
        return self.FramesByUnit[unit]
    end
    
    -- Find an unused frame
    for i, frame in pairs(self.Frames) do
        if not frame.unit then
            frame.unit = unit
            self.FramesByUnit[unit] = frame
            return frame
        end
    end
    
    -- Create new frame if none available
    local frame = self:CreateFrame()
    frame.unit = unit
    table.insert(self.Frames, frame)
    self.FramesByUnit[unit] = frame
    return frame
end

-- Enable frame for a specific unit (by GUID or unit token)
function SYN.Nameplates:EnableUnit(unit)
    self.EnabledUnits[unit] = true
end

-- Disable frame for a specific unit
function SYN.Nameplates:DisableUnit(unit)
    self.EnabledUnits[unit] = nil
    -- Hide frame if it exists
    local frame = self.FramesByUnit[unit]
    if frame then
        frame:Hide()
        frame.unit = nil
        self.FramesByUnit[unit] = nil
    end
end

-- Check if a unit should have a frame shown
function SYN.Nameplates:ShouldShowUnit(unit)
    if self.ShowAll then
        return true
    end
    
    -- Check by unit token
    if self.EnabledUnits[unit] then
        return true
    end
    
    -- Check by GUID
    local guid = UnitGUID(unit)
    if guid and self.EnabledUnits[guid] then
        return true
    end
    
    return false
end

-- Set whether to show all enemy frames or only enabled ones
function SYN.Nameplates:SetShowAll(showAll)
    self.ShowAll = showAll
    self:UpdateFrames()
end

-- Clear all enabled units
function SYN.Nameplates:ClearEnabledUnits()
    for k in pairs(self.EnabledUnits) do
        self.EnabledUnits[k] = nil
    end
end

-- Update nameplate frame positions and visibility
function SYN.Nameplates:UpdateFrames()
    -- Clear active units tracking
    for k in pairs(self.ActiveUnits) do
        self.ActiveUnits[k] = nil
    end
    
    -- Get all visible nameplates
    local nameplates = C_NamePlate.GetNamePlates()
    
    for _, nameplate in pairs(nameplates) do
        local unit = nameplate.namePlateUnitToken
        
        -- Check if unit is an enemy and should be shown
        if unit and UnitExists(unit) and 
           UnitCanAttack("player", unit) and 
           not UnitIsDead(unit) and
           self:ShouldShowUnit(unit) then
            
            -- Get a frame for this nameplate
            local frame = self:GetFrame(unit)
            
            -- Position the frame in the center of the nameplate
            frame:ClearAllPoints()
            frame:SetPoint("CENTER", nameplate, "CENTER", 0, 0)
            
            -- Check if this unit is the mouseover target or player's target
            local isMouseover = (UnitIsUnit(unit, "mouseover") == true)
            local isTarget = (UnitIsUnit(unit, "target") == true)
            
            -- Update border color based on mouseover/target status
            -- Priority: target > mouseover > default
            if isTarget then
                -- Green for target
                frame.Backdrop:SetBackdropBorderColor(0.0, 1.0, 0.0, 1.0)
                frame.Texture:SetColorTexture(0.0, 1.0, 0.0, 0.4)
            elseif isMouseover then
                -- Yellow for mouseover
                frame.Backdrop:SetBackdropBorderColor(1.0, 1.0, 0.0, 1.0)
                frame.Texture:SetColorTexture(1.0, 1.0, 0.0, 0.5)
            else
                -- Blue for default
                frame.Backdrop:SetBackdropBorderColor(0.2, 0.6, 1.0, 0.5)
                frame.Texture:SetColorTexture(0.2, 0.6, 1.0, 0.3)
            end
            
            frame:Show()
            self.ActiveUnits[unit] = true
        end
    end
    
    -- Hide frames for units that are no longer active
    for unit, frame in pairs(self.FramesByUnit) do
        if not self.ActiveUnits[unit] then
            frame:Hide()
            frame.unit = nil
            self.FramesByUnit[unit] = nil
        end
    end
end

-- Initialize the nameplate system
function SYN.Nameplates:Init()
    if self.UpdateFrame then
        return -- Already initialized
    end
    
    -- Create update frame that runs on every frame
    self.UpdateFrame = CreateFrame("Frame")
    self.UpdateFrame:SetScript("OnUpdate", function()
        self:UpdateFrames()
    end)
    
    -- Register for nameplate events
    self.UpdateFrame:RegisterEvent("NAME_PLATE_UNIT_ADDED")
    self.UpdateFrame:RegisterEvent("NAME_PLATE_UNIT_REMOVED")
    self.UpdateFrame:RegisterEvent("UPDATE_MOUSEOVER_UNIT")
    self.UpdateFrame:RegisterEvent("PLAYER_TARGET_CHANGED")
    
    self.UpdateFrame:SetScript("OnEvent", function(_, event, ...)
        if event == "NAME_PLATE_UNIT_ADDED" or 
           event == "NAME_PLATE_UNIT_REMOVED" or
           event == "UPDATE_MOUSEOVER_UNIT" or
           event == "PLAYER_TARGET_CHANGED" then
            self:UpdateFrames()
        end
    end)
    
    print("Synecdoche Nameplate Frames initialized")
end

-- Hide all nameplate frames
function SYN.Nameplates:HideAll()
    for _, frame in pairs(self.Frames) do
        frame:Hide()
    end
end


--
-- -- Show frames only on your current target and mouseover
-- SYN.Nameplates:SetShowAll(false)
-- SYN.Nameplates:EnableUnit("target")
-- SYN.Nameplates:EnableUnit("mouseover")

-- -- Access a specific frame by unit
-- local frame = SYN.Nameplates.FramesByUnit["nameplate1"]
-- if frame then
--     -- Modify this specific frame
--     frame.Texture:SetColorTexture(1, 0, 0, 0.5)  -- Make it red
-- end

-- -- Get all currently active units
-- for unit, _ in pairs(SYN.Nameplates.ActiveUnits) do
--     print("Frame active for: " .. unit)
-- end
--