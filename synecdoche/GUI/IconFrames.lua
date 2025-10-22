--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- Synecdoche
local Utils = SYN.Utils
-- Lua
local pairs = pairs
local tostring = tostring

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

--- ================ ICON FRAME DEFINITIONS ================
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
--               ┌────┼───────────┼────┘    └───────────┘       └───────────┘   
--               │    │           │    │                                        
--               │ H  │           │  I │                                        
--               └────┘           └────┘                                        

-- A -- Main suggestion for current target
SYN.MainIconFrame = CreateBaseIconFrame("SYN_MainIconFrame", UIParent)
SYN.MainIconFrame.CooldownFrame = CreateFrame("Cooldown", "SYN_MainIconCooldownFrame", SYN.MainIconFrame, "AR_CooldownFrameTemplate")

-- B -- Prediction "one timestep out" if you cast A or E
SYN.Prediction1Frame = CreateBaseIconFrame("SYN_Prediction1Frame", UIParent)

-- C -- Prediction two steps out
SYN.Prediction2Frame = CreateBaseIconFrame("SYN_Prediction2Frame", UIParent)

-- D -- "What you should be casting if you weren't moving"
SYN.LeftIconFrame = CreateBaseIconFrame("SYN_LeftIconFrame", UIParent)

-- E -- Shows up if you should be casting on non-current target
SYN.TopIconFrame = CreateBaseIconFrame("SYN_TopIconFrame", UIParent)

-- F, G, H, I - Small frames for CDs and off-GCD abilities
-- F: offensive CD, G: defensive CDs, H: off-GCD before main, I: off-gcd after main
SYN.SmallTopLeftFrame = CreateBaseIconFrame("SYN_SmallTopLeftFrame", UIParent)
SYN.SmallTopRightFrame = CreateBaseIconFrame("SYN_SmallTopRightFrame", UIParent)
SYN.SmallBottomLeftFrame = CreateBaseIconFrame("SYN_SmallBottomLeftFrame", UIParent)
SYN.SmallBottomRightFrame = CreateBaseIconFrame("SYN_SmallBottomRightFrame", UIParent)

--- ================ FRAME INITIALIZATION ================

-- A - Main icon
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

