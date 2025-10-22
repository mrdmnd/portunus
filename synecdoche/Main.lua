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

function SYN.ShowAllFramesWithPlaceholders()
    -- Show all frames with placeholder data for testing
    SYN.MainIconFrame:ChangeIcon(1, "Interface\\Icons\\Spell_Holy_WordFortitude", false, false, "1", "Main", false)
    
    SYN.Prediction1Frame:ChangeIcon(2, "Interface\\Icons\\Spell_Shadow_ShadowBolt", false, false, "2", "Pred1", false)
    SYN.Prediction2Frame:ChangeIcon(3, "Interface\\Icons\\Spell_Fire_FlameBolt", false, false, "3", "Pred2", false)
    SYN.LeftIconFrame:ChangeIcon(4, "Interface\\Icons\\Spell_Nature_HealingTouch", false, false, "4", "Moving", false)
    SYN.TopIconFrame:ChangeIcon(5, "Interface\\Icons\\Spell_Frost_FrostBolt", false, false, "5", "OffTarget", false)
    
    SYN.SmallTopLeftFrame:ChangeIcon(6, "Interface\\Icons\\Spell_Shadow_DeathCoil", false, false, "Shift+1", "OffCD", false)
    SYN.SmallTopRightFrame:ChangeIcon(7, "Interface\\Icons\\Spell_Holy_Heal", false, false, "Shift+2", "DefCD", false)
    SYN.SmallBottomLeftFrame:ChangeIcon(8, "Interface\\Icons\\Spell_Nature_Lightning", false, false, "Ctrl+1", "PreGCD", false)
    SYN.SmallBottomRightFrame:ChangeIcon(9, "Interface\\Icons\\Spell_Arcane_Arcane01", false, false, "Ctrl+2", "PostGCD", false)
end