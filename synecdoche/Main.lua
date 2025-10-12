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

        UIFrames = {
            SYN.MainFrame,
            SYN.MainIconFrame,
        }

        C_Timer.After(2, function()
            SYN.MainFrame:UnregisterEvent("ADDON_LOADED")
            print("Welcome to synecdoche.")
            print("This is the Main.lua file in the ADDON_LOADED event handler.")
            -- SYN.PulseInit()
            
            -- Start glow toggle timer
            SYN.StartGlowTimer()
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
