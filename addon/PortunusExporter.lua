local _, Portunus = ...
local LibDeflate = LibStub:GetLibrary("LibDeflate")
print("You've loaded the portunus exporter addon.")

local profile_start_time = GetTimePreciseSec()

function dump(o)
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
-- and only same table keys that have changed since last game update?

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
local update_period = 0.050 -- number of seconds between state updates
local compression_enabled = false
local portunus_pixel_frames = {}
local stringbyte = string.byte
local mathfloor = math.floor

-- A small optimization used to prevent calling SetColorTexture on pixels whose data hasnt changed
-- This is mostly useful in the "uncompressed" version; when we compress we change most pixels on most frames so the check is less prune-y.
local previous_bytes = {}

local function InitializePixels()
    local pixel_index = 0
    for y = 0, frame_height - 1 do
        for x = 0, frame_width - 1 do
            local pixel = Portunus.MainFrame:CreateTexture(nil, "BACKGROUND")
            pixel:SetSize(1, 1)
            pixel:SetPoint("TOPLEFT", x, -y)
            pixel:SetColorTexture(0, 0, 0, 1)
            local byte_index = 3 * pixel_index
            previous_bytes[byte_index + 1] = 0
            previous_bytes[byte_index + 2] = 0
            previous_bytes[byte_index + 3] = 0
            portunus_pixel_frames[pixel_index + 1] = pixel
            pixel_index = pixel_index + 1
        end
    end
end

-- Length of bytes must be a multiple of three here.
-- We store the "compression enabled" flag in r.
-- First pixel stores the number of bytes to decode in our screenshot reader: num_bytes -> g + 256*b
-- This is VERY PERFORMANCE SENSITIVE.
local function UpdatePixels(bytes)
    local num_bytes = #bytes
    local prev = previous_bytes
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
    for pixel_index = 0, pixel_index_max-1 do
        local base_index = 3 * pixel_index
        local r_index = base_index+1
        local g_index = base_index+2
        local b_index = base_index+3
        local r_new = bytes[r_index]
        local g_new = bytes[g_index]
        local b_new = bytes[b_index]
        -- This optimization is probably only good if compression is off, because if compression is on then we expect to change bytes pretty much all the time.
        if r_new ~= prev[r_index] or g_new ~= prev[g_index] or b_new ~= prev[b_index] then
          r = mult * r_new
          g = mult * g_new
          b = mult * b_new
          prev[r_index] = r_new
          prev[g_index] = g_new
          prev[b_index] = b_new
          frames[pixel_index+2]:SetColorTexture(r, g, b, 1) -- the plus two here is because we're offset by one for lua starting at one and then offset by one again because of the first pixel metadata
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
    local bytes = {stringbyte(serialized_message, 1, #serialized_message)}
    local padding = (3 - (#bytes % 3)) % 3
    for i = 1, padding do
        bytes[#bytes + 1] = 0
    end
    return bytes
end

local function TimerCallback()
    profile_start_time = GetTimePreciseSec()
    local game_state = Portunus.GameState.GetGameState()
    print("getting state took ", 1000000*(GetTimePreciseSec() - profile_start_time) .. "ns")

    --print(dump(game_state))

    profile_start_time = GetTimePreciseSec()
    local bytes = MessagepackEncodeTable(game_state)
    print("encoding table took ", 1000000*(GetTimePreciseSec() - profile_start_time) .. "ns")

    profile_start_time = GetTimePreciseSec()
    UpdatePixels(bytes)
    print("rendering pixels took ", 1000000*(GetTimePreciseSec() - profile_start_time) .. "ns")
    print("------------------------")
end

Portunus.MainFrame:SetScript("OnEvent", function (self, Event, Arg1)
    if Event == "ADDON_LOADED" and Arg1 == "PortunusExporter" then
		Portunus.MainFrame:Show()
        InitializePixels()
        C_Timer.NewTicker(update_period, TimerCallback)
        C_Timer.After(2, function() Portunus.MainFrame:UnregisterEvent("ADDON_LOADED") end)
    end
end)