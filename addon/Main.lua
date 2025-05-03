local LibDeflate = LibStub:GetLibrary("LibDeflate")

local mathfloor = math.floor
local mathfrexp = math.frexp
local mathldexp = math.ldexp
local stringbyte = string.byte

-- Configuration variables:
local update_period = 1.00 -- number of seconds between updates. this is a tradeoff between how much data we can send and how much performance we can afford.
local compression_enabled = false -- whether to compress the data before sending.


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
local portunus_pixel_frames = {}
local previous_bytes = {}

-- We initialize the pixel frames here.
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

-- Length of bytes must be a multiple of three here.
-- The first pixel of the frame contains metadata:
--  r = compression enabled flag
--  g = num_bytes % 256
--  b = (num_bytes / 256) % 256
-- This means that the number of bytes to decode in our screenshot reader is num_bytes -> g + 256*b

-- This is VERY PERFORMANCE SENSITIVE; I've tried to optimize where possible. 
-- One thing to note is that we don't call UpdateTexture on every frame; only the ones that are different from last render.
-- This seems like it's a performance win.
local function UpdatePixels(bytes)
    local num_bytes = #bytes
    if num_bytes > max_storable_bytes then
       print("ERROR: attempted to serialize " .. tostring(num_bytes) .. " bytes but max pixel frame capacity is " .. tostring(max_storable_bytes))
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
Portunus.MainFrame:SetSize(frame_width, frame_height)
Portunus.MainFrame:SetFrameStrata("TOOLTIP")
Portunus.MainFrame:SetPoint("TOPLEFT", 0, 0)
Portunus.MainFrame:RegisterEvent("ADDON_LOADED")

-- Encodes a table as an aligned sequence of bytes with length a multiple of three.
local function EncodeTable(table)
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



-- Serialization code here:
-- These functions allow us to serialize data structures into a flat buffer.
-- The general idea is that we modify the buffer by writing a bytes-level representation of the data starting at the offset,
-- then advance the offset, and return the advanced offset.
-- This is fairly perf sensitive.

-- We pre-code the serialization of most primitives here directly, but codegen the serialization for our non-primitive types
-- from schema.rs directly.

-- ############# SERIALIZATION INTO BUFFER ###############
local function serialize_f32(data, offset, buffer)
    local sign = 0
    if data < 0 then
        sign = 1
        data = -data
    end

    local mantissa, exponent = mathfrexp(data)
    if data == 0 then -- Special case for f32 IEEE zero
        mantissa, exponent = 0, 0
    else
        mantissa = (mantissa * 2 - 1) * mathldexp(0.5, 24)
        exponent = exponent + 126
    end

    buffer[offset + 0] = sign * 128 + mathfloor(exponent / 2)
    buffer[offset + 1] = (exponent % 2) * 128 + mathfloor(mantissa / 65536)
    buffer[offset + 2] = mathfloor(mantissa / 256) % 256
    buffer[offset + 3] = mantissa % 256
    return offset + 4
end

local function serialize_u32(data, offset, buffer)
    buffer[offset + 0] = mathfloor((data) % 256)
    buffer[offset + 1] = mathfloor((data / 256) % 256)
    buffer[offset + 2] = mathfloor((data / 65536) % 256)
    buffer[offset + 3] = mathfloor((data / 16777216) % 256)
    return offset + 4
end

local function serialize_u16(data, offset, buffer)
    buffer[offset + 0] = mathfloor((data) % 256)
    buffer[offset + 1] = mathfloor((data / 256) % 256)
    return offset + 2
end

local function serialize_u8(data, offset, buffer)
    buffer[offset] = mathfloor((data) % 256)
    return offset + 1
end

local function serialize_bool(data, offset, buffer)
    buffer[offset] = data and 1 or 0
    return offset + 1
end


-- should probably codegen these serializers later but for now it's manual.
-- CODEGEN_START
local function serialize_cooldown(data, offset, buffer)
    local o = offset
    o = serialize_u32(data.spell_id, o, buffer)
    o = serialize_u32(data.time_until_next_charge, o, buffer)
    o = serialize_u8(data.charge_info_flags, o, buffer)
    return o
end

local function serialize_aura(data, offset, buffer)
    local o = offset
    o = serialize_u32(data.spell_id, o, buffer)
    o = serialize_u32(data.source_guid_hash, o, buffer)
    o = serialize_u32(data.expiration_time_ms, o, buffer)
    o = serialize_u8(data.flags, o, buffer)
    return o
end

local function serialize_resource(data, offset, buffer)
    local o = offset
    o = serialize_u16(data.current_value, o, buffer)
    o = serialize_u16(data.maximum_value, o, buffer)
    o = serialize_u8(data.resource_type, o, buffer)
    return o
end

local function serialize_unit_base(data, offset, buffer)
    local o = offset
    o = serialize_u32(data.health_current, o, buffer)
    o = serialize_u32(data.health_maximum, o, buffer)
    o = serialize_f32(data.speed, o, buffer)
    o = serialize_u32(data.spell_cast_spell_id, o, buffer)
    o = serialize_u32(data.spell_cast_start_time_ms, o, buffer)
    o = serialize_u32(data.spell_cast_end_time_ms, o, buffer)
    o = serialize_u8(data.spell_cast_flags, o, buffer)
    o = serialize_u32(#data.auras, o, buffer) -- Serialize the number of elements into the buffer, then the elements themselves.
    for _, aura in ipairs(data.auras) do
        o = serialize_aura(aura, o, buffer)
    end
    return o
end

local function serialize_enemy_unit(data, offset, buffer)
    local o = offset
    o = serialize_u32(data.guid_hash, o, buffer)
    o = serialize_u8(data.range, o, buffer)
    o = serialize_u8(data.flags, o, buffer)
    o = serialize_unit_base(data.base, o, buffer)
    return o
end

local function serialize_player_unit(data, offset, buffer)
    local o = offset
    o = serialize_u32(#data.resources, o, buffer)
    for _, resource in ipairs(data.resources) do
        o = serialize_resource(resource, o, buffer)
    end
    o = serialize_u32(#data.cooldowns, o, buffer)
    for _, cooldown in ipairs(data.cooldowns) do
        o = serialize_cooldown(cooldown, o, buffer)
    end
    o = serialize_unit_base(data.base, o, buffer)
    return o
end

local function serialize_snapshot(data, offset, buffer)
    local o = offset
    o = serialize_u32(data.snapshot_time_ms, o, buffer)
    o = serialize_u32(data.combat_time_ms, o, buffer)
    o = serialize_u32(data.target_guid_hash, o, buffer)
    o = serialize_player_unit(data.player, o, buffer)
    o = serialize_u32(#data.enemies, o, buffer)
    for _, enemy in ipairs(data.enemies) do
        o = serialize_enemy_unit(enemy, o, buffer)
    end
    return o
end

-- CODEGEN_END


-- Main Loop Code: every XXXX seconds, this runs.
local function MainLoop()
    --local profile_start_time = GetTimePreciseSec()
    local game_state = Portunus.ExportGameState()
    --print("getting state took ", 1000000*(GetTimePreciseSec() - profile_start_time) .. "us")
    --profile_start_time = GetTimePreciseSec()
    local bytes = EncodeTable(game_state)
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
        -- To do a single run, uncomment the line below and comment out the line after that that runs MainLoop every `update_period`.
        --MainLoop()
        C_Timer.NewTicker(update_period, MainLoop)
        C_Timer.After(2, function() Portunus.MainFrame:UnregisterEvent("ADDON_LOADED") end)
    end
end)