--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...
-- Lua
local print = print
local pairs = pairs
local tonumber = tonumber
local tostring = tostring
local string = string
local table = table

--- ================= UNIFIED SLASH COMMAND SYSTEM =================
-- All commands follow the pattern: /syn <component> <action> <args>

--- Helper function to parse arguments
local function parseArgs(msg)
    local args = {}
    for token in string.gmatch(msg, "%S+") do 
        args[#args + 1] = token 
    end
    return args
end

--- Helper function to normalize category names
local function normalizeCategory(cat)
    local lc = string.lower(cat or "")
    if lc == "movement" then return lc end
    if lc == "defensive" then return lc end
    if lc == "burst" then return lc end
    if lc == "immune" then return lc end
    if lc == "untargetable" then return "immune" end
    return lc
end

SLASH_SYNECDOCHE1 = "/syn"
SLASH_SYNECDOCHE2 = "/synecdoche"

SlashCmdList["SYNECDOCHE"] = function(msg)
    msg = msg:lower():match("^%s*(.-)%s*$") -- Trim whitespace
    local args = parseArgs(msg)
    local component = args[1] or "help"
    
    -- ============================================
    -- BATTLEFIELD COMMANDS
    -- ============================================
    if component == "battlefield" or component == "bf" then
        if SYN.Battlefield then
            local debugInfo = SYN.Battlefield:GetDebugInfo()
            print("=== Battlefield State ===")
            print(string.format("Visible Nameplates: %d", debugInfo.nameplateCount))
            print(string.format("Combat Units: %d", debugInfo.combatUnitCount))
            
            if debugInfo.nameplateCount > 0 then
                print("\nNameplates:")
                for unitToken, data in pairs(debugInfo.nameplates) do
                    local rangeStr = "Unknown"
                    if data.minRange and data.maxRange then
                        rangeStr = string.format("%d-%d yds", data.minRange, data.maxRange)
                    elseif data.minRange then
                        rangeStr = string.format(">%d yds", data.minRange)
                    end
                    print(string.format("  %s: %s [%s] (%.1fs ago)", 
                        unitToken, data.name, rangeStr, data.age))
                end
            end
            
            if debugInfo.combatUnitCount > 0 then
                print("\nCombat Units:")
                for guid, data in pairs(debugInfo.combatUnits) do
                    local visFlag = data.hasNameplate and "[VISIBLE]" or "[OFF-SCREEN]"
                    local deadFlag = data.isDead and "[DEAD]" or ""
                    local combatFlag = data.inCombat and "[COMBAT]" or ""
                    local rangeStr = "Unknown"
                    if data.minRange and data.maxRange then
                        rangeStr = string.format("%d-%d yds", data.minRange, data.maxRange)
                    elseif data.minRange then
                        rangeStr = string.format(">%d yds", data.minRange)
                    end
                    print(string.format("  %s: %s/%s HP [%s] %s %s %s (%.1fs ago)",
                        data.name, data.health, data.healthMax, rangeStr,
                        visFlag, combatFlag, deadFlag, data.age))
                end
            end
        else
            print("Battlefield module not loaded")
        end
        return
    end
    
    -- ============================================
    -- TIMELINE COMMANDS
    -- ============================================
    if component == "timeline" or component == "tl" then
        local action = args[2] and string.lower(args[2]) or "list"
        
        if action == "add" then
            -- Usage: /syn timeline add <inSeconds> <duration> <category> <label...>
            if #args < 6 then
                print("Usage: /syn timeline add <inSeconds> <duration> <category> <label...>")
                return
            end
            local inSec = tonumber(args[3]) or 0
            local dur = tonumber(args[4]) or 0
            local cat = normalizeCategory(args[5])
            local label = table.concat(args, " ", 6)
            local id = SYN.Timeline:AddEventIn(inSec, dur, cat, label, "manual")
            print(string.format("Added: %s in %.1fs for %.1fs [%s] id=%s",
                label, inSec, dur, cat, id or "?"))
            return
        end
        
        if action == "clear" then
            SYN.Timeline:ClearAll()
            print("Timeline cleared.")
            return
        end
        
        -- Default: list
        SYN.Timeline:PurgeExpired()
        local events = SYN.Timeline:All()
        print(string.format("Synecdoche timeline (%d upcoming):", #events))
        for i = 1, #events do
            local e = events[i]
            local inSec = e.startAt - GetTime()
            print(string.format("- %s in %.1fs for %.1fs [%s] id=%s",
                e.label or "(no label)", inSec, e.duration or 0,
                e.category or "unknown", e.id or "?"))
        end
        return
    end
    
    -- ============================================
    -- VOICE ANNOUNCER COMMANDS
    -- ============================================
    if component == "voice" or component == "tts" then
        local action = args[2] and string.lower(args[2]) or "status"
        
        if action == "enable" or action == "on" then
            SYN.VoiceAnnouncer:Enable()
            print("Voice Announcer: ENABLED")
            return
        end
        
        if action == "disable" or action == "off" then
            SYN.VoiceAnnouncer:Disable()
            print("Voice Announcer: DISABLED")
            return
        end
        
        if action == "test" then
            local testText = table.concat(args, " ", 3)
            if testText == "" then testText = "Test" end
            SYN.VoiceAnnouncer:Announce(testText)
            print("Speaking: " .. testText)
            return
        end
        
        if action == "category" then
            -- Usage: /syn voice category <name> <on|off>
            if #args < 4 then
                print("Usage: /syn voice category <name> <on|off>")
                return
            end
            local cat = args[3]
            local enabled = string.lower(args[4]) == "on"
            SYN.VoiceAnnouncer:SetCategoryEnabled(cat, enabled)
            print(string.format("Category '%s': %s", cat, enabled and "ENABLED" or "DISABLED"))
            return
        end
        
        -- Default: status
        local config = SYN.VoiceAnnouncer.GetConfig and SYN.VoiceAnnouncer:GetConfig() or {}
        print("Voice Announcer Status:")
        print("  Enabled: " .. tostring(SYN.VoiceAnnouncer:IsEnabled()))
        if config.countdownSteps then
            print("  Countdown steps: " .. table.concat(config.countdownSteps, ", "))
        end
        if config.minTimeBetweenSpeech then
            print("  Min time between speech: " .. tostring(config.minTimeBetweenSpeech) .. "s")
        end
        if config.announcedCategories then
            print("  Categories:")
            for cat, enabled in pairs(config.announcedCategories) do
                print(string.format("    %s: %s", cat, enabled and "ON" or "OFF"))
            end
        end
        return
    end
    
    -- ============================================
    -- STATE CACHE COMMANDS
    -- ============================================
    if component == "cache" or component == "state" then
        local action = args[2] and string.lower(args[2]) or "dump"
        
        if action == "refresh" then
            SYN.StateCache:Refresh()
            print("State cache refreshed")
            return
        end
        
        if action == "dump" or action == "show" then
            local c = SYN.StateCache:Get()
            print("========== StateCache Dump ==========")
            print("Initialized: " .. tostring(c.initialized))
            print("Timestamp: " .. tostring(c.timestamp))
            
            -- Spec info
            if c.spec then
                print("Spec: " .. tostring(c.spec.specName or "unknown") .. 
                      " (" .. tostring(c.spec.className or "unknown") .. 
                      ") [ID: " .. tostring(c.spec.specId) .. "]")
            else
                print("Spec: nil")
            end
            print("Role: " .. tostring(c.role or "nil"))
            
            -- Spells
            local spellCount = 0
            for _ in pairs(c.spells) do spellCount = spellCount + 1 end
            print("Spells: " .. spellCount .. " known")
            
            -- Spell IDs (first 10 as example)
            local spellIdCount = 0
            local spellIdSample = {}
            for id in pairs(c.spellIds) do
                spellIdCount = spellIdCount + 1
                if #spellIdSample < 10 then
                    table.insert(spellIdSample, tostring(id))
                end
            end
            print("Spell IDs: " .. spellIdCount .. " total (sample: " .. 
                  table.concat(spellIdSample, ", ") .. ")")
            
            -- Talents
            local talentCount = 0
            for _ in pairs(c.talents) do talentCount = talentCount + 1 end
            print("Talents: " .. talentCount .. " selected")
            
            -- Talent Spell IDs
            local talentSpellCount = 0
            local talentSpellSample = {}
            for id in pairs(c.talentSpellIds) do
                talentSpellCount = talentSpellCount + 1
                if #talentSpellSample < 10 then
                    table.insert(talentSpellSample, tostring(id))
                end
            end
            print("Talent Spell IDs: " .. talentSpellCount .. " total (sample: " .. 
                  table.concat(talentSpellSample, ", ") .. ")")
            
            -- Hero Tree
            print("Hero Tree: " .. tostring(c.heroTree or "nil"))
            
            -- Gear
            local gearCount = 0
            for slot, item in pairs(c.gear) do
                gearCount = gearCount + 1
            end
            print("Gear: " .. gearCount .. " items equipped")
            
            -- Stats
            if c.stats then
                print("Stats:")
                for key, value in pairs(c.stats) do
                    print("  " .. key .. ": " .. tostring(value))
                end
            else
                print("Stats: nil")
            end
            
            print("=====================================")
            return
        end
        
        print("Unknown cache action. Try: dump, refresh")
        return
    end
    
    -- ============================================
    -- ENEMY TRACKER COMMANDS
    -- ============================================
    if component == "enemytracker" or component == "et" then
        if SYN.EnemyTrackerFrame and SYN.EnemyTrackerFrame.UpdateBars then
            SYN.EnemyTrackerFrame:UpdateBars()
            print("Enemy tracker updated")
            print("Note: Enemy tracker shows enemies your party is in combat with")
        else
            print("Enemy tracker not initialized")
        end
        return
    end
    
    -- ============================================
    -- DEBUG/TEST COMMANDS
    -- ============================================
    if component == "test" then
        if SYN.Debug and SYN.Debug.ShowAllFramesWithPlaceholders then
            SYN.Debug.ShowAllFramesWithPlaceholders()
        else
            print("Debug module not loaded")
        end
        return
    end
    
    -- ============================================
    -- ENGINE COMMANDS
    -- ============================================
    if component == "reload" then
        SYN.InitializeSpecEngine()
        print("Spec engine reloaded")
        return
    end
    
    -- ============================================
    -- HELP
    -- ============================================
    if component == "help" or component == "" then
        print("=== Synecdoche Commands ===")
        print("/syn battlefield (bf) - Show battlefield state")
        print("/syn timeline (tl) <action>")
        print("  add <seconds> <duration> <category> <label> - Add timeline event")
        print("  clear - Clear all timeline events")
        print("  list - List all timeline events (default)")
        print("/syn voice (tts) <action>")
        print("  enable/disable - Toggle voice announcements")
        print("  test <text> - Speak text")
        print("  category <name> <on|off> - Enable/disable category")
        print("  status - Show voice config (default)")
        print("/syn cache (state) <action>")
        print("  dump - Show cached state (default)")
        print("  refresh - Force refresh cache")
        print("/syn enemytracker (et) - Update enemy tracker")
        print("/syn test - Show test frames")
        print("/syn reload - Reload spec engine")
        print("/syn help - Show this help")
        return
    end
    
    print("Unknown command: " .. component)
    print("Type '/syn help' for available commands.")
end

-- Legacy command aliases (redirect to /syn)
SLASH_SYNTIMELINE1 = "/syntimeline"
SlashCmdList["SYNTIMELINE"] = function(msg)
    print("Note: /syntimeline is deprecated. Use '/syn timeline' instead.")
    SlashCmdList["SYNECDOCHE"]("timeline " .. (msg or ""))
end

SLASH_SYNVOICE1 = "/synvoice"
SLASH_SYNVOICE2 = "/syntts"
SlashCmdList["SYNVOICE"] = function(msg)
    print("Note: /synvoice is deprecated. Use '/syn voice' instead.")
    SlashCmdList["SYNECDOCHE"]("voice " .. (msg or ""))
end

SLASH_DUMPSTATE1 = "/dumpstate"
SLASH_DUMPSTATE2 = "/dumpcache"
SlashCmdList["DUMPSTATE"] = function(msg)
    print("Note: /dumpstate is deprecated. Use '/syn cache dump' instead.")
    SlashCmdList["SYNECDOCHE"]("cache dump")
end
