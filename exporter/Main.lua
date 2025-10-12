local LibDeflate = LibStub:GetLibrary("LibDeflate")

local mathfloor = math.floor
local mathfrexp = math.frexp
local mathldexp = math.ldexp
local stringbyte = string.byte

-- Configuration variables:
-- Number of seconds between updates.
-- This is a tradeoff between staleness and performance.
local UPDATE_PERIOD_SECONDS = 1.00 

-- NOTE: 127x127 seems to hit some hard limits around 128^2 pixels. You can do 128x126 also.
-- on my machine we cap out at ~16160 pixels after some exhaustive /reload binary searching xD
-- this factors nicely as 101x160.
-- we should try NOT to hit this limit as it crashes the UI whenever you mouseover something;
-- instead try something like 33x331 for now, which gives us exactly 32kib + 1 (metadata, num_bytes) of transfer.
-- works out kind of nicely.
-- if we need more storage we can ramp up towards probably a max of 16160 pixel textures.
local FRAME_WIDTH = 33
local FRAME_HEIGHT = 331
local MAX_STORABLE_BYTES = FRAME_WIDTH * FRAME_HEIGHT * 3



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

local portunus_pixel_frames = {}
local previous_bytes = {}

-- We initialize the pixel frames here.
local function InitializePixels()
    for y = 0, FRAME_HEIGHT - 1 do
        for x = 0, FRAME_WIDTH - 1 do
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

-- Length of bytes must be a multiple of three here.
-- The first pixel in the frame contains metadata:
--  r = compression enabled flag
--  g = num_bytes % 256
--  b = (num_bytes / 256) % 256
-- This means that the number of bytes to decode in our screenshot reader is num_bytes -> g + 256*b

-- This is VERY PERFORMANCE SENSITIVE; I've tried to optimize where possible. 
-- One thing to note is that we don't call UpdateTexture on every frame; only the ones that are different from last render.
-- This seems like it's a performance win.
local function UpdatePixels(bytes)
    local num_bytes = #bytes
    if num_bytes > MAX_STORABLE_BYTES then
       print("ERROR: attempted to serialize " .. tostring(num_bytes) .. " bytes but max pixel frame capacity is " .. tostring(MAX_STORABLE_BYTES))
       return 
    end
    local mult = 1.0 / 255.0
    local frames = portunus_pixel_frames
    local pixel_index_max = num_bytes / 3
    -- Set first pixel with metadata.
    local metadata_r = compression_enabled and 1.0 or 0.0
    local metadata_g = mult * (num_bytes % 256)
    local metadata_b = mult * (mathfloor(num_bytes / 256) % 256)
    frames[1]:SetColorTexture(metadata_r, metadata_g, metadata_b, 1)
    -- Set rest of pixels with payload.
    local prev = previous_bytes
    for pixel_index = 0, pixel_index_max-1 do
        local base_index = 3 * pixel_index
        local r_new = bytes[base_index+1]
        local g_new = bytes[base_index+2]
        local b_new = bytes[base_index+3]
        if r_new ~= prev[base_index+1] or g_new ~= prev[base_index+2] or b_new ~= prev[base_index+3] then
            prev[base_index+1] = r_new
            prev[base_index+2] = g_new
            prev[base_index+3] = b_new
            -- the plus two offset below is because we're offset by one for lua starting at one and then offset by one again because of the first pixel metadata
            frames[pixel_index+2]:SetColorTexture(mult * bytes[base_index+1], mult * bytes[base_index+2], mult * bytes[base_index+3], 1)
        end
    end
end

Portunus.MainFrame = CreateFrame("Frame", "PortunusExporter_MainFrame", UIParent)
Portunus.MainFrame:SetSize(FRAME_WIDTH, FRAME_HEIGHT)
Portunus.MainFrame:SetFrameStrata("TOOLTIP")
Portunus.MainFrame:SetPoint("TOPLEFT", 0, 0)
Portunus.MainFrame:RegisterEvent("ADDON_LOADED")

-- Encodes a table as an aligned sequence of bytes with length a multiple of three.
-- Uses dynamic compression if the message is too large to fit in the frame.
-- Returns nil if the table is too large to fit in the frame.
local function EncodeTable(table)
    local raw_message = Portunus.MessagePack.pack(table)
    local raw_length = #raw_message
    local compression_enabled = raw_length > MAX_STORABLE_BYTES
    local serialized_message = compression_enabled and LibDeflate:CompressZlib(raw_message) or raw_message
    local length = #serialized_message
    if length > MAX_STORABLE_BYTES then
        print("ERROR: attempted to encode table " .. dump(table) .. " but max pixel frame capacity is " .. tostring(MAX_STORABLE_BYTES) .. " bytes and the compressed message is " .. tostring(length) .. " bytes")
        return
    end
    local bytes = {}
    -- ISSUE: if length is too big, calling stringbyte will cause a stack overflow; so just call the function if we're sure it'll be okay.
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



-- Main Loop Code: every XXXX seconds, this runs.
local function MainLoop()
    print("------------------------")
    local start_time = GetTimePreciseSec()
    local game_state = Portunus.ExportGameState()
    local end_time = GetTimePreciseSec()
    print("ExportGameState took ", 1000000*(end_time - start_time) .. "us")

    start_time = GetTimePreciseSec()
    local bytes = EncodeTable(game_state)
    end_time = GetTimePreciseSec()
    print("EncodeTable took ", 1000000*(end_time - start_time) .. "us")

    start_time = GetTimePreciseSec()
    UpdatePixels(bytes)
    end_time = GetTimePreciseSec()
    print("UpdatePixels took ", 1000000*(end_time - start_time) .. "us")
    print("------------------------")
end

Portunus.MainFrame:SetScript("OnEvent", function (self, Event, Arg1)
    if Event == "ADDON_LOADED" and Arg1 == "PortunusExporter" then
		Portunus.MainFrame:Show()
        InitializePixels()
        -- To do a single run, uncomment the line below and comment out the line after that that runs MainLoop every `update_period`.
        --MainLoop()
        C_Timer.NewTicker(update_period, MainLoop)
        C_Timer.After(2, function() Portunus.MainFrame:UnregisterEvent("ADDON_LOADED") end)
    end
end)