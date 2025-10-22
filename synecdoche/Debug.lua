--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- Lua
local print = print

--- ================= DEBUG MODULE =================
SYN.Debug = SYN.Debug or {}

--- Show all frames with placeholder data for testing
function SYN.Debug.ShowAllFramesWithPlaceholders()
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
    
    -- Force nameplate frames to show (they will automatically track real nameplates)
    if SYN.Nameplates and SYN.Nameplates.UpdateFrames then
        SYN.Nameplates:UpdateFrames()
        print("Nameplate frames updated - hover over enemies to see mouseover highlighting")
    end
    
    print("All frames displayed with placeholder data")
end

