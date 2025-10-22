--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- Lua
local pairs = pairs

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

--- ================ USAGE EXAMPLES ================
-- 
-- -- Show frames only on your current target and mouseover
-- SYN.Nameplates:SetShowAll(false)
-- SYN.Nameplates:EnableUnit("target")
-- SYN.Nameplates:EnableUnit("mouseover")
--
-- -- Access a specific frame by unit
-- local frame = SYN.Nameplates.FramesByUnit["nameplate1"]
-- if frame then
--     -- Modify this specific frame
--     frame.Texture:SetColorTexture(1, 0, 0, 0.5)  -- Make it red
-- end
--
-- -- Get all currently active units
-- for unit, _ in pairs(SYN.Nameplates.ActiveUnits) do
--     print("Frame active for: " .. unit)
-- end

