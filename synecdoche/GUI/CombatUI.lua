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

-- A -- this is the main suggestion for your current target
SYN.MainIconFrame = CreateFrame("Frame", "SYN_MainIconFrame", UIParent)
SYN.MainIconFrame.CooldownFrame = CreateFrame("Cooldown", "SYN_MainIconCooldownFrame", SYN.MainIconFrame, "AR_CooldownFrameTemplate")

-- B -- this is the prediction "one timestep out" if you cast the thing in A or E (it presumes you choose E if E is rendered)
SYN.Prediction1Frame = CreateFrame("Frame", "SYN_Prediction1Frame", UIParent)

-- C
SYN.Prediction2Frame = CreateFrame("Frame", "SYN_Prediction2Frame", UIParent)

-- D -- this is the "what you should be casting if you weren't moving" icon; alternatively, this is what you'd switch to cast if you finished moving right this moment

SYN.LeftIconFrame = CreateFrame("Frame", "SYN_LeftIconFrame", UIParent)

-- E -- this shows up if you should be casting something on your non-current target. the main icon will fade slightly red if this shows up
SYN.TopIconFrame = CreateFrame("Frame", "SYN_TopIconFrame", UIParent)

-- F, G, H, I
-- Mostly allocate these for offensive CD (F), defensive CDs (G), off-GCD that shoudl be hit before main icon (H), and off-gcd that should be hit after main icon (I)
SYN.SmallTopLeftFrame = CreateFrame("Frame", "SYN_SmallTopLeftFrame", UIParent)
SYN.SmallTopRightFrame = CreateFrame("Frame", "SYN_SmallTopRightFrame", UIParent)
SYN.SmallBottomLeftFrame = CreateFrame("Frame", "SYN_SmallBottomLeftFrame", UIParent)
SYN.SmallBottomRightFrame = CreateFrame("Frame", "SYN_SmallBottomRightFrame", UIParent)

function SYN.ResetIcons()
    -- A
    SYN.MainIconFrame:Hide()
    SYN.MainIconFrame.Backdrop:Hide()
    -- B
    SYN.Prediction1Frame:HideIcon()
    SYN.Prediction1Frame.Backdrop:Hide()
    -- C
    SYN.Prediction2Frame:HideIcon()
    SYN.Prediction2Frame.Backdrop:Hide()

    -- D
    SYN.LeftIconFrame:HideIcon()
    SYN.LeftIconFrame.Backdrop:Hide()

    -- E
    SYN.TopIconFrame:HideIcon()
    SYN.TopIconFrame.Backdrop:Hide()

    -- F
    SYN.SmallTopLeftFrame:HideIcon()
    SYN.SmallTopLeftFrame.Backdrop:Hide()

    -- G
    SYN.SmallTopRightFrame:HideIcon()
    SYN.SmallTopRightFrame.Backdrop:Hide()

    -- H
    SYN.SmallBottomLeftFrame:HideIcon()
    SYN.SmallBottomLeftFrame.Backdrop:Hide()

    -- I
    SYN.SmallBottomRightFrame:HideIcon()
    SYN.SmallBottomRightFrame.Backdrop:Hide()
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
    -- MainFrame has a Texture, a CooldownFrame (the swirl), a KeybindFrame, a TextFrame, 
    -- Frame init
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(64)
    self:SetHeight(64)
    self:SetPoint("CENTER", SYN.MainFrame, "CENTER", 0, 0)
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    -- Cooldown
    self.CooldownFrame:SetAllPoints(self)

    -- Keybind on main icon
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 14, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("=)")

    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 8, "OUTLINE")
    AnnotationFrame:SetText("Placeholder")

    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)

    self:Show()
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
function SYN.MainIconFrame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 14, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 8, "OUTLINE")

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

function SYN.MainIconFrame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

function SYN.MainIconFrame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

--- ================ FRAME INITIALIZATION ================

-- B - Prediction1Frame (right of main icon)
function SYN.Prediction1Frame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(64)
    self:SetHeight(64)
    self:SetPoint("LEFT", SYN.MainIconFrame, "RIGHT", 5, 0)
    
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    
    -- Cooldown
    self.CooldownFrame = CreateFrame("Cooldown", nil, self, "AR_CooldownFrameTemplate")
    self.CooldownFrame:SetAllPoints(self)
    
    -- Keybind
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 14, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 8, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)
    
    self:Hide()
end

-- C - Prediction2Frame (right of prediction1)
function SYN.Prediction2Frame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(64)
    self:SetHeight(64)
    self:SetPoint("LEFT", SYN.Prediction1Frame, "RIGHT", 5, 0)
    
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    
    -- Cooldown
    self.CooldownFrame = CreateFrame("Cooldown", nil, self, "AR_CooldownFrameTemplate")
    self.CooldownFrame:SetAllPoints(self)
    
    -- Keybind
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 14, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 8, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)
    
    self:Hide()
end

-- D - LeftIconFrame (left of main icon)
function SYN.LeftIconFrame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(64)
    self:SetHeight(64)
    self:SetPoint("RIGHT", SYN.MainIconFrame, "LEFT", -5, 0)
    
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    
    -- Cooldown
    self.CooldownFrame = CreateFrame("Cooldown", nil, self, "AR_CooldownFrameTemplate")
    self.CooldownFrame:SetAllPoints(self)
    
    -- Keybind
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 14, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 8, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)
    
    self:Hide()
end

-- E - TopIconFrame (above main icon)
function SYN.TopIconFrame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(64)
    self:SetHeight(64)
    self:SetPoint("BOTTOM", SYN.MainIconFrame, "TOP", 0, 5)
    
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    
    -- Cooldown
    self.CooldownFrame = CreateFrame("Cooldown", nil, self, "AR_CooldownFrameTemplate")
    self.CooldownFrame:SetAllPoints(self)
    
    -- Keybind
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 14, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 8, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)
    
    self:Hide()
end

-- F - SmallTopLeftFrame (top-left corner of main icon)
function SYN.SmallTopLeftFrame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(32)
    self:SetHeight(32)
    self:SetPoint("BOTTOMRIGHT", SYN.MainIconFrame, "TOPLEFT", 0, 0)
    
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    
    -- Cooldown
    self.CooldownFrame = CreateFrame("Cooldown", nil, self, "AR_CooldownFrameTemplate")
    self.CooldownFrame:SetAllPoints(self)
    
    -- Keybind
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 10, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 6, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)
    
    self:Hide()
end

-- G - SmallTopRightFrame (top-right corner of main icon)
function SYN.SmallTopRightFrame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(32)
    self:SetHeight(32)
    self:SetPoint("BOTTOMLEFT", SYN.MainIconFrame, "TOPRIGHT", 0, 0)
    
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    
    -- Cooldown
    self.CooldownFrame = CreateFrame("Cooldown", nil, self, "AR_CooldownFrameTemplate")
    self.CooldownFrame:SetAllPoints(self)
    
    -- Keybind
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 10, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 6, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)
    
    self:Hide()
end

-- H - SmallBottomLeftFrame (bottom-left corner of main icon)
function SYN.SmallBottomLeftFrame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(32)
    self:SetHeight(32)
    self:SetPoint("TOPRIGHT", SYN.MainIconFrame, "BOTTOMLEFT", 0, 0)
    
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    
    -- Cooldown
    self.CooldownFrame = CreateFrame("Cooldown", nil, self, "AR_CooldownFrameTemplate")
    self.CooldownFrame:SetAllPoints(self)
    
    -- Keybind
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 10, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 6, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)
    
    self:Hide()
end

-- I - SmallBottomRightFrame (bottom-right corner of main icon)
function SYN.SmallBottomRightFrame:Init()
    self:SetFrameStrata(SYN.MainFrame:GetFrameStrata())
    self:SetFrameLevel(SYN.MainFrame:GetFrameLevel() - 1)
    self:SetWidth(32)
    self:SetHeight(32)
    self:SetPoint("TOPLEFT", SYN.MainIconFrame, "BOTTOMRIGHT", 0, 0)
    
    -- Texture
    self.Texture = self:CreateTexture(nil, "ARTWORK")
    self.Texture:SetTexture("Interface\\Icons\\Spell_Holy_WordFortitude")
    self.Texture:SetAllPoints(self)
    
    -- Cooldown
    self.CooldownFrame = CreateFrame("Cooldown", nil, self, "AR_CooldownFrameTemplate")
    self.CooldownFrame:SetAllPoints(self)
    
    -- Keybind
    local KeybindFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Keybind = KeybindFrame
    KeybindFrame:SetFont(SYN.FontSelect(KeybindFrame), 10, "OUTLINE")
    KeybindFrame:SetAllPoints(true)
    KeybindFrame:SetJustifyH("RIGHT")
    KeybindFrame:SetJustifyV("TOP")
    KeybindFrame:SetPoint("TOPRIGHT")
    KeybindFrame:SetTextColor(0.8, 0.8, 0.8, 1)
    KeybindFrame:SetText("")
    
    -- Annotation Text
    local AnnotationFrame = self:CreateFontString(nil, "OVERLAY", "GameFontHighlight")
    self.Annotation = AnnotationFrame
    AnnotationFrame:SetAllPoints(true)
    AnnotationFrame:SetJustifyH("CENTER")
    AnnotationFrame:SetJustifyV("MIDDLE")
    AnnotationFrame:SetPoint("CENTER")
    AnnotationFrame:SetTextColor(1, 1, 1, 1)
    AnnotationFrame:SetFont(SYN.FontSelect(AnnotationFrame), 6, "OUTLINE")
    AnnotationFrame:SetText("")
    
    -- Set black border
    self.Texture:SetTexCoord(0.01, 0.92, 0.08, 0.92)
    SYN.CreateBackdrop(self)
    
    self:Hide()
end

--- ================ CHANGE ICON METHODS ================

-- B - Prediction1Frame ChangeIcon
function SYN.Prediction1Frame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 14, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 8, "OUTLINE")

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

-- C - Prediction2Frame ChangeIcon
function SYN.Prediction2Frame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 14, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 8, "OUTLINE")

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

-- D - LeftIconFrame ChangeIcon
function SYN.LeftIconFrame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 14, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 8, "OUTLINE")

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

-- E - TopIconFrame ChangeIcon
function SYN.TopIconFrame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 14, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 8, "OUTLINE")

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

-- F - SmallTopLeftFrame ChangeIcon
function SYN.SmallTopLeftFrame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 10, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 6, "OUTLINE")

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

-- G - SmallTopRightFrame ChangeIcon
function SYN.SmallTopRightFrame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 10, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 6, "OUTLINE")

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

-- H - SmallBottomLeftFrame ChangeIcon
function SYN.SmallBottomLeftFrame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 10, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 6, "OUTLINE")

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

-- I - SmallBottomRightFrame ChangeIcon
function SYN.SmallBottomRightFrame:ChangeIcon(ID, Texture, Unusable, OutOfRange, Keybind, Annotation, Glow)
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

    self.Keybind:SetFont(SelectedFont, 10, "OUTLINE")
    self.Annotation:SetFont(SelectedFont, 6, "OUTLINE")

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

--- ================ COOLDOWN AND ID METHODS ================

-- B - Prediction1Frame SetCooldown
function SYN.Prediction1Frame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

-- B - Prediction1Frame GetIconID
function SYN.Prediction1Frame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

-- C - Prediction2Frame SetCooldown
function SYN.Prediction2Frame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

-- C - Prediction2Frame GetIconID
function SYN.Prediction2Frame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

-- D - LeftIconFrame SetCooldown
function SYN.LeftIconFrame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

-- D - LeftIconFrame GetIconID
function SYN.LeftIconFrame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

-- E - TopIconFrame SetCooldown
function SYN.TopIconFrame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

-- E - TopIconFrame GetIconID
function SYN.TopIconFrame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

-- F - SmallTopLeftFrame SetCooldown
function SYN.SmallTopLeftFrame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

-- F - SmallTopLeftFrame GetIconID
function SYN.SmallTopLeftFrame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

-- G - SmallTopRightFrame SetCooldown
function SYN.SmallTopRightFrame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

-- G - SmallTopRightFrame GetIconID
function SYN.SmallTopRightFrame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

-- H - SmallBottomLeftFrame SetCooldown
function SYN.SmallBottomLeftFrame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

-- H - SmallBottomLeftFrame GetIconID
function SYN.SmallBottomLeftFrame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

-- I - SmallBottomRightFrame SetCooldown
function SYN.SmallBottomRightFrame:SetCooldown(CooldownStart, CooldownDuration)
    if CooldownStart == 0 or CooldownDuration == 0 then
        self.CooldownFrame:SetCooldown(0, 0)
        self.CooldownFrame:Hide()
        return
    end
    self.CooldownFrame:SetCooldown(CooldownStart, CooldownDuration)
end

-- I - SmallBottomRightFrame GetIconID
function SYN.SmallBottomRightFrame:GetIconID()
    if self.ID then
        return self.ID
    end
    return nil
end

--- ================ HIDE ICON METHODS ================

-- B - Prediction1Frame HideIcon
function SYN.Prediction1Frame:HideIcon()
    self:Hide()
    self.Backdrop:Hide()
    ActionButton_HideOverlayGlow(self)
end

-- C - Prediction2Frame HideIcon
function SYN.Prediction2Frame:HideIcon()
    self:Hide()
    self.Backdrop:Hide()
    ActionButton_HideOverlayGlow(self)
end

-- D - LeftIconFrame HideIcon
function SYN.LeftIconFrame:HideIcon()
    self:Hide()
    self.Backdrop:Hide()
    ActionButton_HideOverlayGlow(self)
end

-- E - TopIconFrame HideIcon
function SYN.TopIconFrame:HideIcon()
    self:Hide()
    self.Backdrop:Hide()
    ActionButton_HideOverlayGlow(self)
end

-- F - SmallTopLeftFrame HideIcon
function SYN.SmallTopLeftFrame:HideIcon()
    self:Hide()
    self.Backdrop:Hide()
    ActionButton_HideOverlayGlow(self)
end

-- G - SmallTopRightFrame HideIcon
function SYN.SmallTopRightFrame:HideIcon()
    self:Hide()
    self.Backdrop:Hide()
    ActionButton_HideOverlayGlow(self)
end

-- H - SmallBottomLeftFrame HideIcon
function SYN.SmallBottomLeftFrame:HideIcon()
    self:Hide()
    self.Backdrop:Hide()
    ActionButton_HideOverlayGlow(self)
end

-- I - SmallBottomRightFrame HideIcon
function SYN.SmallBottomRightFrame:HideIcon()
    self:Hide()
    self.Backdrop:Hide()
    ActionButton_HideOverlayGlow(self)
end

--- ================ fuckin' everything else lol ================


--- ================ NAMEPLATES ==================
SYN.Nameplates = {
    MainInitialized = false,
    SuggestedInitialized = false,
}

-- function SYN.Nameplate.AddIcon(Unit, Object)
--     local Token = stringlower(Unit.UnitID)
--     if not Token then return false end
--     local Nameplate = C_NamePlate.GetNamePlateForUnit(Token)
--     if not Nameplate then return false end
--     -- Scale things to screen, basically
--     local ScreenHeight = GetScreenHeight()
--     local NameplateScaler = (ScreenHeight > 768) and (768 / ScreenHeight) or 1
--     local NameplateIconSize = Nameplate:GetHeight() / NameplateScaler
--     local HealthBar = Nameplate.UnitFrame.healthBar
--     NameplateIconSize = (HealthBar:GetWidth() / NameplateScaler)

--     local IconFrame = SYN.NameplateIconFrame1
--     --- got bored, come back to line 560 on UI.lua on hr
-- end

-- function SYN.Nameplate.HideIcons()
--     SYN.NameplateIconFrame:Hide()
--     SYN.NameplateSuggestedIconFrame:Hide()
-- end