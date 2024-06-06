local _, Portunus = ...

-- We can't use "require" so we've gotta do it manually.
Portunus.Modules = {}


local LibDeflate = LibStub:GetLibrary("LibDeflate")

local function dump(o)
   if type(o) == 'table' then
      local s = '{ '
      for k,v in pairs(o) do
         if type(k) ~= 'number' then k = '"'..k..'"' end
         s = s .. '['..k..'] = ' .. dump(v) .. ','
      end
      return s .. '} '
   else
      return tostring(o)
   end
end

-- Potential future idea: 
-- Only send Deltas (incremental updates) over our bandwidth limited channel. We maintain a copy of the last updated full game state
-- and only send table key/values that have changed since last game update?

-- NOTE: 127x127 seems to hit some hard limits around 128^2 pixels. You can do 128x126 also.
-- on my machine we cap out at ~16160 pixels after some exhaustive /reload binary searching xD
-- this factors nicely as 101x160.
-- we should try NOT to hit this limit as it crashes the UI whenever you mouseover something;
-- instead try something like 33x331 for now, which gives us exactly 32kib + 1 (metadata, num_bytes) of transfer.
-- works out kind of nicely.
-- if we need more storage we can ramp up towards probably a max of 16160 pixel textures.
local frame_width = 33
local frame_height = 331
local max_storable_bytes = frame_width * frame_height * 3
local update_period = 1.00
local compression_enabled = false
local portunus_pixel_frames = {}
local stringbyte = string.byte
local mathfloor = math.floor

local previous_bytes = {}
local function InitializePixels()
    for y = 0, frame_height - 1 do
        for x = 0, frame_width - 1 do
            local pixel = Portunus.MainFrame:CreateTexture(nil, "BACKGROUND")
            pixel:SetSize(1, 1)
            pixel:SetPoint("TOPLEFT", x, -y)
            pixel:SetColorTexture(0, 0, 0, 1)
            previous_bytes[#previous_bytes+1] = 0
            previous_bytes[#previous_bytes+1] = 0
            previous_bytes[#previous_bytes+1] = 0
            portunus_pixel_frames[#portunus_pixel_frames + 1] = pixel
        end
    end
end

-- Load whitelists on a per-spec / per-class basis.
Portunus.PlayerAuraWhitelist = {}
Portunus.TargetAuraWhitelist = {}
Portunus.PlayerCooldownWhitelist = {}
local function InitializeClassSpecificWhitelists()
    local EnabledRotation = {
    -- Death Knight
      --[250]   = "HeroRotation_DeathKnight",   -- Blood
      --[251]   = "HeroRotation_DeathKnight",   -- Frost
      --[252]   = "HeroRotation_DeathKnight",   -- Unholy
    -- Demon Hunter
      --[577]   = "HeroRotation_DemonHunter",   -- Havoc
      --[581]   = "HeroRotation_DemonHunter",   -- Vengeance
    -- Druid
      --[102]   = "HeroRotation_Druid",         -- Balance
      --[103]   = "HeroRotation_Druid",         -- Feral
      --[104]   = "HeroRotation_Druid",         -- Guardian
      --[105]   = "HeroRotation_Druid",         -- Restoration
    -- Evoker
        [1467]  = "classes_evoker_Devastation", -- Devastation
      --[1468] = "HeroRotation_Evoker",         -- Preservation
      --[1473]  = "HeroRotation_Evoker",        -- Augmentation
    -- Hunter
      --[253]   = "HeroRotation_Hunter",        -- Beast Mastery
      --[254]   = "HeroRotation_Hunter",        -- Marksmanship
      --[255]   = "HeroRotation_Hunter",        -- Survival
    -- Mage
      --[62]    = "HeroRotation_Mage",          -- Arcane
      --[63]    = "HeroRotation_Mage",          -- Fire
      --[64]    = "HeroRotation_Mage",          -- Frost
    -- Monk
      --[268]   = "HeroRotation_Monk",          -- Brewmaster
      --[269]   = "HeroRotation_Monk",          -- Windwalker
      --[270]   = "HeroRotation_Monk",          -- Mistweaver
    -- Paladin
      --[65]    = "HeroRotation_Paladin",       -- Holy
      --[66]    = "HeroRotation_Paladin",       -- Protection
      --[70]    = "HeroRotation_Paladin",       -- Retribution
    -- Priest
      --[256]   = "HeroRotation_Priest",        -- Discipline
      --[257]   = "HeroRotation_Priest",        -- Holy
        [258]   = "classes_priest_Shadow",      -- Shadow
    -- Rogue
      --[259]   = "HeroRotation_Rogue",         -- Assassination
      --[260]   = "HeroRotation_Rogue",         -- Outlaw
      --[261]   = "HeroRotation_Rogue",         -- Subtlety
    -- Shaman
      --[262]   = "HeroRotation_Shaman",        -- Elemental
      --[263]   = "HeroRotation_Shaman",        -- Enhancement
      --[264]   = "HeroRotation_Shaman",        -- Restoration
    -- Warlock
      --[265]   = "HeroRotation_Warlock",       -- Affliction
      --[266]   = "HeroRotation_Warlock",       -- Demonology
      --[267]   = "HeroRotation_Warlock",       -- Destruction
    -- Warrior
      --[71]    = "HeroRotation_Warrior",       -- Arms
      --[72]    = "HeroRotation_Warrior",       -- Fury
      --[73]    = "HeroRotation_Warrior"        -- Protection
  };
    -- TODO: implement me.
end

-- Length of bytes must be a multiple of three here.
-- We store the "compression enabled" flag in r.
-- First pixel stores the number of bytes to decode in our screenshot reader: num_bytes -> g + 256*b
-- This is VERY PERFORMANCE SENSITIVE; I've tried to optimize where possible. One thing to note is that we don't call UpdateTexture on every frame; only the ones that are different from last render.
local function UpdatePixels(bytes)
    local num_bytes = #bytes
    if num_bytes > max_storable_bytes then print("ERROR: attempted to serialize " .. tostring(num_bytes) .. " bytes but max capacity is " .. tostring(max_storable_bytes)) return end
    local mult = 1.0 / 255.0
    local frames = portunus_pixel_frames
    local pixel_index_max = num_bytes / 3
    -- Set first pixel with metadata.
    local r = compression_enabled and 1.0 or 0.0
    local g = mult * (num_bytes % 256)
    local b = mult * (mathfloor(num_bytes / 256) % 256)
    frames[1]:SetColorTexture(r, g, b, 1)
    -- Set rest of pixels with payload.
    local prev = previous_bytes
    for pixel_index = 0, pixel_index_max-1 do
        local base_index = 3 * pixel_index
        local r_index = base_index+1
        local g_index = base_index+2
        local b_index = base_index+3
        local r_new = bytes[r_index]
        local g_new = bytes[g_index]
        local b_new = bytes[b_index]
        if r_new ~= prev[r_index] or g_new ~= prev[g_index] or b_new ~= prev[b_index] then
            prev[r_index] = r_new
            prev[g_index] = g_new
            prev[b_index] = b_new
            -- the plus two offset below is because we're offset by one for lua starting at one and then offset by one again because of the first pixel metadata
            frames[pixel_index+2]:SetColorTexture(mult * bytes[r_index], mult * bytes[g_index], mult * bytes[b_index], 1)
        end
    end
end

Portunus.MainFrame = CreateFrame("Frame", "PortunusExporter_MainFrame", UIParent)
Portunus.MainFrame:SetSize(frame_width, frame_height)
Portunus.MainFrame:SetFrameStrata("TOOLTIP")
Portunus.MainFrame:SetPoint("TOPLEFT", 0, 0)
Portunus.MainFrame:RegisterEvent("ADDON_LOADED")

-- Uses MessagePack to encode a table as an aligned sequence of bytes with length a multiple of three.
local function MessagepackEncodeTable(table)
    local raw_message = Portunus.MessagePack.pack(table)
    -- TODO: dynamically turn on compression ONLY WHEN we need it (i.e. the message is too large for the window)
    -- Turns out it's better to do less CPU work in the compressor I think.
    ---@diagnostic disable-next-line: undefined-field
    local serialized_message = compression_enabled and LibDeflate:CompressZlib(raw_message) or raw_message
    local length = #serialized_message
    local bytes = {}
    -- ISSUE: this causes a stack overflow if length is too big; so just call the function if we're sure it'll be okay.
    -- Chose the current threshold somewhat experimentally on my client.
    if length < 2048 then
        bytes = {stringbyte(serialized_message, 1, length)}
    else
        local chunk_size = 2048
        local byte_count = 0
        for i = 1, length, chunk_size do
            local end_idx = math.min(i + chunk_size - 1, length)
            local chunk = {stringbyte(serialized_message, i, end_idx)}
            for _, byte in ipairs(chunk) do
                byte_count = byte_count + 1
                bytes[byte_count] = byte
            end
        end
    end
    local padding = (3 - (#bytes % 3)) % 3
    for i = 1, padding do
        bytes[#bytes + 1] = 0
    end
    return bytes
end

local function TimerCallback()
    --local profile_start_time = GetTimePreciseSec()
    local game_state = Portunus.ExportGameState()
    --print("getting state took ", 1000000*(GetTimePreciseSec() - profile_start_time) .. "us")

    --print(dump(game_state))

    --profile_start_time = GetTimePreciseSec()
    local bytes = MessagepackEncodeTable(game_state)
    --print("encoding table took ", 1000000*(GetTimePreciseSec() - profile_start_time) .. "us")

    --profile_start_time = GetTimePreciseSec()
    UpdatePixels(bytes)
    --print("rendering pixels took ", 1000000*(GetTimePreciseSec() - profile_start_time) .. "us")

    --print("------------------------")
end

Portunus.MainFrame:SetScript("OnEvent", function (self, Event, Arg1)
    if Event == "ADDON_LOADED" and Arg1 == "PortunusExporter" then
		Portunus.MainFrame:Show()
        InitializePixels()
        --InitializeClassSpecificWhitelists()
        TimerCallback()
        --C_Timer.NewTicker(update_period, TimerCallback)
        C_Timer.After(2, function() Portunus.MainFrame:UnregisterEvent("ADDON_LOADED") end)
    end
end)