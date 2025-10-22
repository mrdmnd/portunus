--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- Lua
local pairs = pairs

--- ================ UI HELPER FUNCTIONS ================

--- Creates a backdrop/border for a frame
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

--- Selects the appropriate font for a frame
function SYN.FontSelect(Frame)
    return GameFontNormal:GetFont()
end

--- Resets all icon frames to hidden state
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
    
    -- Timeline bar icons are managed by UpdateIcons; hide background/visuals
    if SYN.TimelineBarFrame.Bg then SYN.TimelineBarFrame.Bg:Hide() end
    if SYN.TimelineBarFrame.Backdrop then SYN.TimelineBarFrame.Backdrop:Hide() end
    if SYN.TimelineBarFrame.ZeroMarker then SYN.TimelineBarFrame.ZeroMarker:Hide() end
    
    -- Enemy tracker reset
    if SYN.EnemyTrackerFrame and SYN.EnemyTrackerFrame.Reset then
        SYN.EnemyTrackerFrame:Reset()
    end
end

