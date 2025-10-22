--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- SynLib
-- Lua
local print = print
local mathmax = math.max
local mathmin = math.min
local pairs = pairs
local select = select

-- LibRangeCheck
local rc = LibStub("LibRangeCheck-3.0")


-- File Locals
local IsAddOnLoaded = C_AddOns.IsAddOnLoaded
local LoadAddOn = C_AddOns.LoadAddOn
local UIFrames
local PrevResult, CurrResult
local CurrentEngine = nil
local UpdateTicker = nil
local SPEC_ELEMENTAL = 1
local SPEC_ENHANCEMENT = 2
local SPEC_RESTORATION = 3



--- ================ CONTENTS ================

SYN.MainFrame = CreateFrame("Frame", "SYN_MainFrame", UIParent)
SYN.MainFrame:SetFrameStrata("HIGH")
SYN.MainFrame:SetFrameLevel(10)
SYN.MainFrame:SetWidth(112)
SYN.MainFrame:SetHeight(96)
SYN.MainFrame:SetClampedToScreen(true)

SYN.MainFrame:RegisterEvent("ADDON_LOADED")
SYN.MainFrame:RegisterEvent("PLAYER_SPECIALIZATION_CHANGED")
SYN.MainFrame:SetScript("OnEvent", function (self, Event, Arg1)
    if Event == "ADDON_LOADED" and Arg1 == "Synecdoche" then
        SYN.MainFrame:SetPoint("CENTER", UIParent, "CENTER", 0, 0)
        SYN.MainFrame:SetFrameStrata("HIGH")
        SYN.MainFrame:Show()
        SYN.MainIconFrame:Init()
        
        -- Initialize all the new frames
        SYN.Prediction1Frame:Init()
        SYN.Prediction2Frame:Init()
        SYN.LeftIconFrame:Init()
        SYN.TopIconFrame:Init()
        SYN.SmallTopLeftFrame:Init()
        SYN.SmallTopRightFrame:Init()
        SYN.SmallBottomLeftFrame:Init()
        SYN.SmallBottomRightFrame:Init()
        SYN.TimelineBarFrame:Init()
        SYN.EnemyTrackerFrame:Init()

        UIFrames = {
            SYN.MainFrame,
            SYN.MainIconFrame,
            SYN.Prediction1Frame,
            SYN.Prediction2Frame,
            SYN.LeftIconFrame,
            SYN.TopIconFrame,
            SYN.SmallTopLeftFrame,
            SYN.SmallTopRightFrame,
            SYN.SmallBottomLeftFrame,
            SYN.SmallBottomRightFrame,
        }

        C_Timer.After(2, function()
            SYN.MainFrame:UnregisterEvent("ADDON_LOADED")
            print("Welcome to synecdoche's combat addon.")
            print("This is the Main.lua file in the ADDON_LOADED event handler.")

            -- Initialize slowly-changing state cache (talents/spells/gear/stats)
            if SYN.StateCache and SYN.StateCache.Init then
                SYN.StateCache:Init()
                print("Synecdoche StateCache initialized")
            end

            -- Initialize Timeline and hooks
            if SYN.Timeline and SYN.Timeline.Init then
                SYN.Timeline:Init()
                print("Synecdoche Timeline initialized")
            end
            
            -- Initialize Voice Announcer
            if SYN.VoiceAnnouncer and SYN.VoiceAnnouncer.Init then
                SYN.VoiceAnnouncer:Init()
                print("Synecdoche Voice Announcer initialized")
            end
            
            -- Initialize Battlefield state tracker
            if SYN.Battlefield and SYN.Battlefield.Init then
                SYN.Battlefield:Init()
                print("Synecdoche Battlefield tracker initialized")
            end
            
            -- Initialize Nameplate Frames
            if SYN.Nameplates and SYN.Nameplates.Init then
                SYN.Nameplates:Init()
            end
            
            -- Initialize spec engine
            SYN.InitializeSpecEngine()
            
        end)
    elseif Event == "PLAYER_SPECIALIZATION_CHANGED" then
        -- Handle spec changes
        print("Spec changed detected, reloading engine...")
        SYN.InitializeSpecEngine()
    end
end)

-- Initialize or switch the spec-specific engine
function SYN.InitializeSpecEngine()
    local spec = GetSpecialization()
    
    -- Delay if spec is not available yet
    if spec == nil then
        C_Timer.After(0.5, function()
            SYN.InitializeSpecEngine()
        end)
        return
    end
    
    -- Cleanup old engine
    SYN.StopUpdateTicker()
    if CurrentEngine and CurrentEngine.Cleanup then
        CurrentEngine:Cleanup()
    end
    CurrentEngine = nil
    
    -- Load the appropriate engine based on spec
    local specName = "Unknown"
    if spec == SPEC_ELEMENTAL then
        specName = "Elemental"
        CurrentEngine = SYN.Elemental
    elseif spec == SPEC_ENHANCEMENT then
        specName = "Enhancement"
        -- CurrentEngine = SYN.Enhancement (not implemented yet)
    elseif spec == SPEC_RESTORATION then
        specName = "Restoration"
        -- CurrentEngine = SYN.Restoration (not implemented yet)
    end
    
    print("Loading " .. specName .. " Shaman engine...")
    
    -- Initialize the new engine
    if CurrentEngine and CurrentEngine.Init then
        CurrentEngine:Init()
        SYN.StartUpdateTicker()
    else
        print("Warning: No engine available for " .. specName .. " spec")
    end
end

-- Start the 30ms update ticker
function SYN.StartUpdateTicker()
    if UpdateTicker then
        UpdateTicker:Cancel()
    end
    
    UpdateTicker = C_Timer.NewTicker(0.03, function() -- 30ms = 0.03 seconds
        -- Update Battlefield state tracker
        if SYN.Battlefield and SYN.Battlefield.Update then
            SYN.Battlefield:Update()
        end
        
        -- Update Timeline Bar
        if SYN.TimelineBarFrame and SYN.TimelineBarFrame.UpdateIcons then
            SYN.TimelineBarFrame:UpdateIcons()
        end
        
        -- Update Enemy Tracker
        if SYN.EnemyTrackerFrame and SYN.EnemyTrackerFrame.UpdateBars then
            SYN.EnemyTrackerFrame:UpdateBars()
        end
        
        -- Update current spec engine
        if CurrentEngine and CurrentEngine.Update then
            CurrentEngine:Update()
        end
    end)
    
    print("Update ticker started (30ms interval)")
end

-- Stop the update ticker
function SYN.StopUpdateTicker()
    if UpdateTicker then
        UpdateTicker:Cancel()
        UpdateTicker = nil
        print("Update ticker stopped")
    end
end

-- Test function and slash commands have been moved to Debug.lua and Commands.lua