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



--- ================ CONTENTS ================

SYN.MainFrame = CreateFrame("Frame", "SYN_MainFrame", UIParent)
SYN.MainFrame:SetFrameStrata("HIGH")
SYN.MainFrame:SetFrameLevel(10)
SYN.MainFrame:SetWidth(112)
SYN.MainFrame:SetHeight(96)
SYN.MainFrame:SetClampedToScreen(true)

SYN.MainFrame:RegisterEvent("ADDON_LOADED")
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
            print("Welcome to synecdoche.")
            print("This is the Main.lua file in the ADDON_LOADED event handler.")
            -- SYN.PulseInit()
            
            -- Show all frames with placeholder data for testing
            SYN.ShowAllFramesWithPlaceholders()
            
            -- Start glow toggle timer
            SYN.StartGlowTimer()
            
            -- Demo LibRangeCheck
            print("LibRangeCheck demo: Use /synrange command to test range checking")
            if rc then
                print("LibRangeCheck-3.0 loaded successfully!")
            else
                print("Warning: LibRangeCheck-3.0 failed to load")
            end
        end)
    end
end)

function SYN.PulseInit()
    -- local Spec = GetSpecialization()
    -- Delay by a second until the API returns a valid value.
    -- if Spec == nil then
    --     C_Timer.After(1, function()
    --         SYN.PulseInit()
    --     end
    --     )
    -- else
    --     -- Force a refresh of everything from the core.
    --     print("cool we got the spec")
    -- end
end

function SYN.ShowAllFramesWithPlaceholders()
    -- Show all frames with placeholder data for testing
    SYN.MainIconFrame:ChangeIcon(1, "Interface\\Icons\\Spell_Holy_WordFortitude", false, false, "1", "Main", false)
    
    SYN.Prediction1Frame:ChangeIcon(2, "Interface\\Icons\\Spell_Shadow_ShadowBolt", false, false, "2", "Pred1", false)
    SYN.Prediction2Frame:ChangeIcon(3, "Interface\\Icons\\Spell_Fire_FlameBolt", false, false, "3", "Pred2", false)
    SYN.LeftIconFrame:ChangeIcon(4, "Interface\\Icons\\Spell_Nature_HealingTouch", false, false, "4", "Left", false)
    SYN.TopIconFrame:ChangeIcon(5, "Interface\\Icons\\Spell_Frost_FrostBolt", false, false, "5", "Top", false)
    
    SYN.SmallTopLeftFrame:ChangeIcon(6, "Interface\\Icons\\Spell_Shadow_DeathCoil", false, false, "Shift+1", "OffCD", false)
    SYN.SmallTopRightFrame:ChangeIcon(7, "Interface\\Icons\\Spell_Holy_Heal", false, false, "Shift+2", "DefCD", false)
    SYN.SmallBottomLeftFrame:ChangeIcon(8, "Interface\\Icons\\Spell_Nature_Lightning", false, false, "Ctrl+1", "PreGCD", false)
    SYN.SmallBottomRightFrame:ChangeIcon(9, "Interface\\Icons\\Spell_Arcane_Arcane01", false, false, "Ctrl+2", "PostGCD", false)
end

function SYN.StartGlowTimer()
    local glowState = false
    
    local function toggleGlow()
        glowState = not glowState
        if SYN.MainIconFrame then
            if glowState then
                ActionButton_ShowOverlayGlow(SYN.MainIconFrame)
                print("Glow ON")
            else
                ActionButton_HideOverlayGlow(SYN.MainIconFrame)
                print("Glow OFF")
            end
        end
        
        -- Schedule next toggle in 5 seconds
        C_Timer.After(5, toggleGlow)
    end
    
    -- Start the first toggle after 5 seconds
    C_Timer.After(5, toggleGlow)
end

-- LibRangeCheck demonstration function
function SYN.DemoRangeCheck()
    if not rc then
        print("LibRangeCheck not loaded!")
        return
    end
    
    local target = "target"
    if not UnitExists(target) then
        print("No target selected for range check")
        return
    end
    
    local minRange, maxRange = rc:GetRange(target)
    if not minRange then
        print("Cannot determine range to target")
    elseif not maxRange then
        print("Target is > " .. minRange .. " yards away")
    else
        print("Target is in (" .. minRange .. ", " .. maxRange .. "] yards away")
    end
end



-- Debug command to test range checking
SLASH_SYNRANGE1 = "/synrange"
SlashCmdList["SYNRANGE"] = function(msg)
    SYN.DemoRangeCheck()
end
