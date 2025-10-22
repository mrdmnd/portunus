--- ================= HEADER =================
--- Text-to-Speech announcement system for timeline events and suggestions

-- KNOWN IMPORTANT THING TO DO:
-- https://claude.ai/share/66612ae6-4b19-43c0-a7b0-a11f6b3bb82b
-- /tts playline (disable the tick sound)
-- /tts playactivity (also disable the tick sound)

--- ======== LOCALIZE =========
local addonName, SYN = ...

-- File locals
local VoiceAnnouncer = {}
local activeAnnouncements = {}  -- Tracks what we're currently announcing
local announcementHistory = {}  -- Prevents duplicate announcements

-- ======== GLOBALIZE =========
SYN.VoiceAnnouncer = VoiceAnnouncer

--- ================ CONFIGURATION ================
local CONFIG = {
    enabled = true,
    -- Which countdown intervals to announce (in seconds)
    countdownSteps = {5, 4, 3, 2, 1},
    -- Which categories should trigger announcements
    announcedCategories = {
        defensive = true,
        immune = true,
        burst = true,
        movement = false,  -- Usually too spammy
    },
    -- Minimum duration to announce (ignore very short events)
    minDurationToAnnounce = 0.5,
    -- Minimum time between announcements to prevent overlap (seconds)
    minTimeBetweenSpeech = 0.3,
    -- Use TTS if available, otherwise print to chat
    useTTS = true,
    -- Voice settings (for C_VoiceChat.SpeakText)
    voiceID = 3,  -- 0 = default system voice
    volume = 100,
    rate = 5,
}

--- ================ HELPERS ================
local function now()
    return GetTime()
end

local function shouldAnnounceCategory(category)
    if not CONFIG.announcedCategories then return false end
    return CONFIG.announcedCategories[category] == true
end

local function createAnnouncementKey(eventId, step)
    return tostring(eventId) .. ":" .. tostring(step)
end

local function hasBeenAnnounced(eventId, step)
    local key = createAnnouncementKey(eventId, step)
    return announcementHistory[key] == true
end

local function markAsAnnounced(eventId, step)
    local key = createAnnouncementKey(eventId, step)
    announcementHistory[key] = true
end

local function cleanupAnnouncementHistory()
    -- Clear history for events that are no longer active
    local currentTime = now()
    for key, _ in pairs(announcementHistory) do
        -- Extract event ID from key
        local eventId = string.match(key, "^(.+):%d+$")
        if eventId then
            local stillActive = false
            for id, data in pairs(activeAnnouncements) do
                if id == eventId and data.expiresAt > currentTime then
                    stillActive = true
                    break
                end
            end
            if not stillActive then
                announcementHistory[key] = nil
            end
        end
    end
end

--- ================ ANNOUNCEMENT ================
local lastSpeakTime = 0

-- Speak text immediately with cooldown protection
-- Note: WoW's TTS queue is unreliable (plays out of order), so we use immediate mode
local function speak(text)
    if not CONFIG.enabled then return end
    if not text or text == "" then return end
    
    local currentTime = now()
    
    -- Enforce cooldown to prevent overlap
    if currentTime - lastSpeakTime < CONFIG.minTimeBetweenSpeech then
        return
    end
    
    lastSpeakTime = currentTime
    
    if CONFIG.useTTS and C_VoiceChat and C_VoiceChat.SpeakText then
        C_VoiceChat.SpeakText(
            CONFIG.voiceID,
            text,
            Enum.VoiceTtsDestination.LocalPlayback,
            CONFIG.rate,
            CONFIG.volume
        )
    else
        -- Fallback: print to chat with distinctive formatting
        print("|cFF00FF00[TTS]|r " .. text)
    end
end

--- ================ API ================
function VoiceAnnouncer:Init()
    -- Hook into timeline updates
    self:StartUpdateLoop()
end

function VoiceAnnouncer:Enable()
    CONFIG.enabled = true
end

function VoiceAnnouncer:Disable()
    CONFIG.enabled = false
end

function VoiceAnnouncer:IsEnabled()
    return CONFIG.enabled
end

function VoiceAnnouncer:SetCountdownSteps(steps)
    CONFIG.countdownSteps = steps
end

function VoiceAnnouncer:SetCategoryEnabled(category, enabled)
    CONFIG.announcedCategories[category] = enabled
end

function VoiceAnnouncer:SetMinTimeBetweenSpeech(seconds)
    CONFIG.minTimeBetweenSpeech = seconds
end

function VoiceAnnouncer:StartUpdateLoop()
    -- Create a hidden frame to drive updates
    if not self.updateFrame then
        self.updateFrame = CreateFrame("Frame")
        self.updateFrame:SetScript("OnUpdate", function(_, elapsed)
            self:Update()
        end)
    end
end

function VoiceAnnouncer:Update()
    if not CONFIG.enabled then return end
    if not SYN.Timeline or not SYN.Timeline.All then return end
    
    local currentTime = now()
    local events = SYN.Timeline:All()
    
    -- Update active announcements
    for i = 1, #events do
        local ev = events[i]
        
        -- Check if this event should be announced
        if shouldAnnounceCategory(ev.category) and 
           (ev.duration or 0) >= CONFIG.minDurationToAnnounce then
            
            local timeUntilStart = (ev.startAt or currentTime) - currentTime
            
            -- Track this event if we haven't seen it
            if not activeAnnouncements[ev.id] then
                activeAnnouncements[ev.id] = {
                    label = ev.label,
                    category = ev.category,
                    startAt = ev.startAt,
                    expiresAt = ev.expiresAt,
                }
            end
            
            -- Check each countdown step
            for idx, step in ipairs(CONFIG.countdownSteps) do
                if not hasBeenAnnounced(ev.id, step) then
                    local tolerance = 0.15
                    
                    -- Speak if we're within the tolerance window
                    if timeUntilStart <= step and timeUntilStart > (step - tolerance) then
                        -- Mark as announced BEFORE speaking to prevent duplicates
                        markAsAnnounced(ev.id, step)
                        
                        local message
                        if idx == 1 then
                            message = (ev.label or ev.category) .. " in " .. tostring(step)
                        else
                            message = tostring(step)
                        end
                        
                        speak(message)
                    end
                end
            end
        end
    end
    
    -- Cleanup expired events from active announcements
    for id, data in pairs(activeAnnouncements) do
        if data.expiresAt < currentTime then
            activeAnnouncements[id] = nil
        end
    end
    
    -- Periodic cleanup of history
    if not self.lastCleanup or (currentTime - self.lastCleanup) > 5 then
        cleanupAnnouncementHistory()
        self.lastCleanup = currentTime
    end
end

-- Manual announcement (for suggestion engine to call)
-- Keep text SHORT - WoW's TTS queue is unreliable for longer messages
function VoiceAnnouncer:Announce(text)
    speak(text)
end

--- ============ DEBUG / COMMANDS ============
SLASH_SYNVOICE1 = "/synvoice"
SLASH_SYNVOICE2 = "/syntts"
SlashCmdList["SYNVOICE"] = function(msg)
    local text = msg or ""
    local args = {}
    for token in string.gmatch(text, "%S+") do args[#args + 1] = token end
    local cmd = args[1] and string.lower(args[1]) or "status"
    
    if cmd == "enable" or cmd == "on" then
        VoiceAnnouncer:Enable()
        print("Voice Announcer: ENABLED")
        return
    end
    
    if cmd == "disable" or cmd == "off" then
        VoiceAnnouncer:Disable()
        print("Voice Announcer: DISABLED")
        return
    end
    
    if cmd == "test" then
        local testText = table.concat(args, " ", 2)
        if testText == "" then testText = "Test" end
        VoiceAnnouncer:Announce(testText)
        return
    end
    
    if cmd == "category" then
        -- Usage: /synvoice category defensive off
        if #args < 3 then
            print("Usage: /synvoice category <name> <on|off>")
            return
        end
        local cat = args[2]
        local enabled = string.lower(args[3]) == "on"
        VoiceAnnouncer:SetCategoryEnabled(cat, enabled)
        print(string.format("Category '%s': %s", cat, enabled and "ENABLED" or "DISABLED"))
        return
    end
    
    -- Default: status
    print("Voice Announcer Status:")
    print("  Enabled: " .. tostring(CONFIG.enabled))
    print("  Countdown steps: " .. table.concat(CONFIG.countdownSteps, ", "))
    print("  Min time between speech: " .. tostring(CONFIG.minTimeBetweenSpeech) .. "s")
    print("  Speech rate: " .. tostring(CONFIG.rate) .. "x")
    print("  Volume: " .. tostring(CONFIG.volume))
    print("  Categories:")
    for cat, enabled in pairs(CONFIG.announcedCategories) do
        print(string.format("    %s: %s", cat, enabled and "ON" or "OFF"))
    end
    print("Commands:")
    print("  /synvoice enable|disable")
    print("  /synvoice test [message]")
    print("  /synvoice category <name> on|off")
    print("Note: Keep announcements SHORT - WoW TTS queue is unreliable")
end

