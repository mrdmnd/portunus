-- Implements customer serialization functions for our data types.

-- ############# PRIMITIVE SERIALIZATION INTO BUFFER ###############

-- Serialize a single F32 into the buffer at the given offset.
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

-- Serialize a single U32 into the buffer at the given offset.
local function serialize_u32(data, offset, buffer)
    buffer[offset + 0] = mathfloor((data) % 256)
    buffer[offset + 1] = mathfloor((data / 256) % 256)
    buffer[offset + 2] = mathfloor((data / 65536) % 256)
    buffer[offset + 3] = mathfloor((data / 16777216) % 256)
    return offset + 4
end

-- Serialize a single U16 into the buffer at the given offset.
local function serialize_u16(data, offset, buffer)
    buffer[offset + 0] = mathfloor((data) % 256)
    buffer[offset + 1] = mathfloor((data / 256) % 256)
    return offset + 2
end

-- Serialize a single U8 into the buffer at the given offset.
local function serialize_u8(data, offset, buffer)
    buffer[offset] = mathfloor((data) % 256)
    return offset + 1
end

-- Serialize a single bool into the buffer at the given offset.
local function serialize_bool(data, offset, buffer)
    buffer[offset] = data and 1 or 0
    return offset + 1
end

-- Serialize a single string into the buffer at the given offset.
-- In this case, we first serialize the length of the string, then the string itself.
-- The string is treated as a sequence of bytes.
local function serialize_string(data, offset, buffer)
    local o = offset
    local length = #data
    o = serialize_u32(length, o, buffer)
    for i = 1, length do
        buffer[o + i] = stringbyte(data, i, i)
    end
    return o + length + 1
end

-- ############# SPECIFIC DATATYPE SERIALIZATION INTO BUFFER ###############

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