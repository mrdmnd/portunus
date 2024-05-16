----------------------------------------- BIT 32 MODULE  ----------------------------------
local bit = {}

local P = 2^32
function bit.bnot(x)
	x = x % P
	return P - 1 - x
end
function bit.band(x, y)
	-- Common usecases, they deserve to be optimized
	if y == 0xff then return x % 0x100 end
	if y == 0xffff then return x % 0x10000 end
	if y == 0xffffffff then return x % 0x100000000 end
	x, y = x % P, y % P
	local r = 0
	local p = 1
	for i = 1, 32 do
		local a, b = x % 2, y % 2
		x, y = math.floor(x / 2), math.floor(y / 2)
		if a + b == 2 then
			r = r + p
		end
		p = 2 * p
	end
	return r
end

------------------------------------------ COMPAT MODULE ----------------------------------
local compat_module = {}

compat_module.GetAlignSize = function(k, size) return bit.band(bit.bnot(k) + 1,(size - 1)) end

if not table.unpack then table.unpack = unpack end

if not table.pack then table.pack = pack end

compat_module.string_pack = string.pack
compat_module.string_unpack = string.unpack


------------------------------------------ BINARY ARRAY MODULE ---------------------------
local compat = compat_module
local string_pack = compat.string_pack
local string_unpack = compat.string_unpack


local binary_array_module = {} -- the module table
local binary_array_mt = {} -- the module metatable

-- given a binary array, set a metamethod to return its length
-- (e.g., #binaryArray, calls this)
function binary_array_mt:__len()
    return self.size
end

-- Create a new binary array of an initial size
function binary_array_module.New(sizeOrString)
    -- the array storage itself
    local o = {}
    
    if type(sizeOrString) == "string" then
        o.str = sizeOrString
        o.size = #sizeOrString
    elseif type(sizeOrString) == "number" then
        o.data = {}
        o.size = sizeOrString
    else
        error("Expect a integer size value or string to construct a binary array")
    end
    -- set the inheritance
    setmetatable(o, {__index = binary_array_mt, __len = binary_array_mt.__len})
    return o
end

-- Get a slice of the binary array from start to end position
function binary_array_mt:Slice(startPos, endPos)
    startPos = startPos or 0
    endPos = endPos or self.size
    local d = self.data
    if d then
        -- if the self.data is defined, we are building the buffer
        -- in a Lua table
        
        -- new table to store the slice components
        local b = {}
        
        -- starting with the startPos, put all
        -- values into the new table to be concat later
        -- updated the startPos based on the size of the
        -- value
        while startPos < endPos do
            local v = d[startPos]
            if not v or v == "" then
                v = '/0'
            end
            table.insert(b, v)
            startPos = startPos + #v
        end

        -- combine the table of strings into one string
        -- this is faster than doing a bunch of concats by themselves
        return table.concat(b)
    else
        -- n.b start/endPos are 0-based incoming, so need to convert
        --     correctly. in python a slice includes start -> end - 1
        return self.str:sub(startPos+1, endPos)
    end
end

-- Grow the binary array to a new size, placing the exisiting data at then end of the new array
function binary_array_mt:Grow(newsize)
    -- the new table to store the data
    local newT = {}
    
    -- the offset to be applied to existing entries
    local offset = newsize - self.size
    
    -- loop over all the current entries and
    -- add them to the new table at the correct
    -- offset location
    local d = self.data
    for i,data in pairs(d) do
        newT[i + offset] = data
    end
    
    -- update this storage with the new table and size
    self.data = newT
    self.size = newsize
end

-- memorization for padding strings
local pads = {}

-- pad the binary with n \0 bytes at the starting position
function binary_array_mt:Pad(n, startPos)
    -- use memorization to avoid creating a bunch of strings
    -- all the time
    local s = pads[n]
    if not s then
        s = string.rep('\0', n)
        pads[n] = s
    end
    
    -- store the padding string at the start position in the
    -- Lua table
    self.data[startPos] = s
end

-- Sets the binary array value at the specified position
function binary_array_mt:Set(value, position)
    self.data[position] = value
end

-- Pack the data into a binary representation
function binary_array_module.Pack(fmt, ...)
    return string_pack(fmt, ...)
end

-- Unpack the data from a binary representation in a Lua value
function binary_array_module.Unpack(fmt, s, pos)
    return string_unpack(fmt, s.str, pos + 1)
end

------------------------------------------ NUM TYPES MODULE ------------------------------
local num_types_module = {}

local ba = binary_array_module

local bpack = ba.Pack
local bunpack = ba.Unpack

local type_mt =  {}

function type_mt:Pack(value)
    return bpack(self.packFmt, value)
end

function type_mt:Unpack(buf, pos)    
    return bunpack(self.packFmt, buf, pos)
end

function type_mt:ValidNumber(n)
    if not self.min_value and not self.max_value then return true end
    return self.min_value <= n and n <= self.max_value
end

function type_mt:EnforceNumber(n)
    -- duplicate code since the overhead of function calls 
    -- for such a popular method is time consuming
    if not self.min_value and not self.max_value then 
        return 
    end
    
    if self.min_value <= n and n <= self.max_value then 
        return
    end    
    
    error("Number is not in the valid range") 
end

function type_mt:EnforceNumbers(a,b)
   -- duplicate code since the overhead of function calls
    -- for such a popular method is time consuming
    if not self.min_value and not self.max_value then
        return
    end

    if self.min_value <= a and a <= self.max_value and self.min_value <= b and b <= self.max_value then
        return
    end

    error("Number is not in the valid range")
end

function type_mt:EnforceNumberAndPack(n)
    return bpack(self.packFmt, n)    
end

function type_mt:ConvertType(n, otherType)
    assert(self.bytewidth == otherType.bytewidth, "Cannot convert between types of different widths")
    if self == otherType then
        return n
    end
    return otherType:Unpack(self:Pack(n))
end

local bool_mt =
{
    bytewidth = 1,
    min_value = false,
    max_value = true,
    lua_type = type(true),
    name = "bool",
    packFmt = "<I1",
    Pack = function(self, value) return value and "1" or "0" end,
    Unpack = function(self, buf, pos) return buf[pos] == "1" end,
    ValidNumber = function(self, n) return true end, -- anything is a valid boolean in Lua
    EnforceNumber = function(self, n) end, -- anything is a valid boolean in Lua
    EnforceNumbers = function(self, a, b) end, -- anything is a valid boolean in Lua
    EnforceNumberAndPack = function(self, n) return self:Pack(n) end,
}

local uint8_mt = 
{
    bytewidth = 1,
    min_value = 0,
    max_value = 2^8-1,
    lua_type = type(1),
    name = "uint8",
    packFmt = "<I1"
}

local uint16_mt = 
{
    bytewidth = 2,
    min_value = 0,
    max_value = 2^16-1,
    lua_type = type(1),
    name = "uint16",
    packFmt = "<I2"
}

local uint32_mt = 
{
    bytewidth = 4,
    min_value = 0,
    max_value = 2^32-1,
    lua_type = type(1),
    name = "uint32",
    packFmt = "<I4"
}

local uint64_mt = 
{
    bytewidth = 8,
    min_value = 0,
    max_value = 2^64-1,
    lua_type = type(1),
    name = "uint64",
    packFmt = "<I8"
}

local int8_mt = 
{
    bytewidth = 1,
    min_value = -2^7,
    max_value = 2^7-1,
    lua_type = type(1),
    name = "int8",
    packFmt = "<i1"
}

local int16_mt = 
{
    bytewidth = 2,
    min_value = -2^15,
    max_value = 2^15-1,
    lua_type = type(1),
    name = "int16",
    packFmt = "<i2"
}

local int32_mt = 
{
    bytewidth = 4,
    min_value = -2^31,
    max_value = 2^31-1,
    lua_type = type(1),
    name = "int32",
    packFmt = "<i4"
}

local int64_mt = 
{
    bytewidth = 8,
    min_value = -2^63,
    max_value = 2^63-1,
    lua_type = type(1),
    name = "int64",
    packFmt = "<i8"
}

local float32_mt = 
{
    bytewidth = 4,
    min_value = nil,
    max_value = nil,
    lua_type = type(1.0),
    name = "float32",
    packFmt = "<f"
}

local float64_mt = 
{
    bytewidth = 8,
    min_value = nil,
    max_value = nil,
    lua_type = type(1.0),
    name = "float64",
    packFmt = "<d"
}

-- register the base class
setmetatable(bool_mt, {__index = type_mt})
setmetatable(uint8_mt, {__index = type_mt})
setmetatable(uint16_mt, {__index = type_mt})
setmetatable(uint32_mt, {__index = type_mt})
setmetatable(uint64_mt, {__index = type_mt})
setmetatable(int8_mt, {__index = type_mt})
setmetatable(int16_mt, {__index = type_mt})
setmetatable(int32_mt, {__index = type_mt})
setmetatable(int64_mt, {__index = type_mt})
setmetatable(float32_mt, {__index = type_mt})
setmetatable(float64_mt, {__index = type_mt})


num_types_module.Uint8     = uint8_mt
num_types_module.Uint16    = uint16_mt
num_types_module.Uint32    = uint32_mt
num_types_module.Uint64    = uint64_mt
num_types_module.Int8      = int8_mt
num_types_module.Int16     = int16_mt
num_types_module.Int32     = int32_mt
num_types_module.Int64     = int64_mt
num_types_module.Float32   = float32_mt
num_types_module.Float64   = float64_mt

num_types_module.UOffsetT  = uint32_mt
num_types_module.VOffsetT  = uint16_mt
num_types_module.SOffsetT  = int32_mt

local GenerateTypes = function(listOfTypes)
    for _,t in pairs(listOfTypes) do
        t.Pack = function(self, value) return bpack(self.packFmt, value) end
        t.Unpack = function(self, buf, pos) return bunpack(self.packFmt, buf, pos) end
    end
end

GenerateTypes(num_types_module)
-- explicitly execute after GenerateTypes call, as we don't want to define a Pack/Unpack function for it.
num_types_module.Bool      = bool_mt


------------------------------------------ VIEW MODULE ------------------------------------
local compat = compat_module
local string_unpack = compat.string_unpack

local view_module = {}
local view_mt = {}

local view_mt_name = "flatbuffers.view.mt"

local N = num_types_module
local binaryarray = binary_array_module

local function enforceOffset(off)
    if off < 0 or off > 42949672951 then
        error("Offset is not valid")
    end
end

local function unPackUoffset(bytes, off)
    return string_unpack("<I4", bytes.str, off + 1)
end

local function unPackVoffset(bytes, off)
    return string_unpack("<I2", bytes.str, off + 1)
end

function view_module.New(buf, pos)
    enforceOffset(pos)
    -- need to convert from a string buffer into
    -- a binary array

    local o = {
        bytes = type(buf) == "string" and binaryarray.New(buf) or buf,
        pos = pos,
    }
    setmetatable(o, {__index = view_mt, __metatable = view_mt_name})
    return o
end

function view_mt:Offset(vtableOffset)
    local vtable = self.vtable
    if not vtable then
        vtable = self.pos - self:Get(N.SOffsetT, self.pos)
        self.vtable = vtable
        self.vtableEnd = self:Get(N.VOffsetT, vtable)
    end
    if vtableOffset < self.vtableEnd then
        return unPackVoffset(self.bytes, vtable + vtableOffset)
    end
    return 0
end

function view_mt:Indirect(off)
    enforceOffset(off)
    return off + unPackUoffset(self.bytes, off)
end

function view_mt:String(off)
    enforceOffset(off)
    off = off + unPackUoffset(self.bytes, off)
    local start = off + 4
    local length = unPackUoffset(self.bytes, off)
    return self.bytes:Slice(start, start+length)
end

function view_mt:VectorLen(off)
    enforceOffset(off)
    off = off + self.pos
    off = off + unPackUoffset(self.bytes, off)
    return unPackUoffset(self.bytes, off)
end

function view_mt:Vector(off)
    enforceOffset(off)
    off = off + self.pos
    return off + self:Get(N.UOffsetT, off) + 4
end

function view_mt:VectorAsString(off, start, stop)
    local o = self:Offset(off)
    if o ~= 0 then
        start = start or 1
        stop = stop or self:VectorLen(o)
        local a = self:Vector(o) + start - 1
        return self.bytes:Slice(a, a + stop - start + 1)
    end
    return nil
end

function view_mt:Union(t2, off)
    assert(getmetatable(t2) == view_mt_name)
    enforceOffset(off)
    off = off + self.pos
    t2.pos = off + self:Get(N.UOffsetT, off)
    t2.bytes = self.bytes
end

function view_mt:Get(flags, off)
    enforceOffset(off)
    return flags:Unpack(self.bytes, off)
end

function view_mt:GetSlot(slot, d, validatorFlags)
    N.VOffsetT:EnforceNumber(slot)
    if validatorFlags then
        validatorFlags:EnforceNumber(d)
    end
    local off = self:Offset(slot)
    if off == 0 then
        return d
    end
    return self:Get(validatorFlags, self.pos + off)
end

function view_mt:GetVOffsetTSlot(slot, d)
    N.VOffsetT:EnforceNumbers(slot, d)
    local off = self:Offset(slot)
    if off == 0 then
        return d
    end
    return off
end


------------------------------------------ BUILDER MODULE ----------------------------------
local builder_module = {}

local N = num_types_module
local ba = binary_array_module
local compat = compat_module
local string_unpack = compat.string_unpack

local builder_mt = {}

local VOffsetT  = N.VOffsetT
local UOffsetT  = N.UOffsetT
local SOffsetT  = N.SOffsetT
local Bool      = N.Bool
local Uint8     = N.Uint8
local Uint16    = N.Uint16
local Uint32    = N.Uint32
local Uint64    = N.Uint64
local Int8      = N.Int8
local Int16     = N.Int16
local Int32     = N.Int32
local Int64     = N.Int64
local Float32   = N.Float32
local Float64   = N.Float64

local MAX_BUFFER_SIZE = 0x80000000 -- 2 GB
local VtableMetadataFields = 2

local getAlignSize = compat.GetAlignSize

local function vtableEqual(a, objectStart, b)
    UOffsetT:EnforceNumber(objectStart)
    if (#a * 2) ~= #b then
        return false
    end

    for i, elem in ipairs(a) do
        local x = string_unpack(VOffsetT.packFmt, b, 1 + (i - 1) * 2)
        if x ~= 0 or elem ~= 0 then
            local y = objectStart - elem
            if x ~= y then
                return false
            end
        end
    end
    return true
end

function builder_module.New(initialSize)
    assert(0 <= initialSize and initialSize < MAX_BUFFER_SIZE)
    local o =
    {
        finished = false,
        bytes = ba.New(initialSize),
        nested = false,
        head = initialSize,
        minalign = 1,
        vtables = {}
    }
    setmetatable(o, {__index = builder_mt})
    return o
end

-- Clears the builder and resets the state. It does not actually clear the backing binary array, it just reuses it as
-- needed. This is a performant way to use the builder for multiple constructions without the overhead of multiple
-- builder allocations.
function builder_mt:Clear()
    self.finished = false
    self.nested = false
    self.minalign = 1
    self.currentVTable = nil
    self.objectEnd = nil
    self.head = self.bytes.size -- place the head at the end of the binary array

    -- clear vtables instead of making a new table
    local vtable = self.vtables
    local vtableCount = #vtable
    for i=1,vtableCount do vtable[i] = nil end
end

function builder_mt:Output(full)
    assert(self.finished, "Builder Not Finished")
    if full then
        return self.bytes:Slice()
    else
        return self.bytes:Slice(self.head)
    end
end

function builder_mt:StartObject(numFields)
    assert(not self.nested)

    local vtable = {}

    for _=1,numFields do
        table.insert(vtable, 0)
    end

    self.currentVTable = vtable
    self.objectEnd = self:Offset()
    self.nested = true
end

function builder_mt:WriteVtable()
    self:PrependSOffsetTRelative(0)
    local objectOffset = self:Offset()

    local exisitingVTable
    local i = #self.vtables
    while i >= 1 do
        if self.vtables[i] == 0 then
            table.remove(self.vtables,i)
        end
        i = i - 1
    end

    i = #self.vtables
    while i >= 1 do

        local vt2Offset = self.vtables[i]
        local vt2Start = self.bytes.size - vt2Offset
        local vt2lenstr = self.bytes:Slice(vt2Start, vt2Start+1)
        local vt2Len = string_unpack(VOffsetT.packFmt, vt2lenstr, 1)

        local metadata = VtableMetadataFields * 2
        local vt2End = vt2Start + vt2Len
        local vt2 = self.bytes:Slice(vt2Start+metadata,vt2End)

        if vtableEqual(self.currentVTable, objectOffset, vt2) then
            exisitingVTable = vt2Offset
            break
        end

        i = i - 1
    end

    if not exisitingVTable then
        i = #self.currentVTable
        while i >= 1 do
            local off = 0
            local a = self.currentVTable[i]
            if a and a ~= 0 then
                off = objectOffset - a
            end
            self:PrependVOffsetT(off)

            i = i - 1
        end

        local objectSize = objectOffset - self.objectEnd
        self:PrependVOffsetT(objectSize)

        local vBytes = #self.currentVTable + VtableMetadataFields
        vBytes = vBytes * 2
        self:PrependVOffsetT(vBytes)

        local objectStart = self.bytes.size - objectOffset
        self.bytes:Set(SOffsetT:Pack(self:Offset() - objectOffset),objectStart)

        table.insert(self.vtables, self:Offset())
    else
        local objectStart = self.bytes.size - objectOffset
        self.head = objectStart
        self.bytes:Set(SOffsetT:Pack(exisitingVTable - objectOffset),self.head)
    end

    self.currentVTable = nil
    return objectOffset
end

function builder_mt:EndObject()
    assert(self.nested)
    self.nested = false
    return self:WriteVtable()
end

local function growByteBuffer(self, desiredSize)
    local s = self.bytes.size
    assert(s < MAX_BUFFER_SIZE, "Flat Buffers cannot grow buffer beyond 2 gigabytes")
    local newsize = s
    repeat
        newsize = math.min(newsize * 2, MAX_BUFFER_SIZE)
        if newsize == 0 then newsize = 1 end
    until newsize > desiredSize

    self.bytes:Grow(newsize)
end

function builder_mt:Head()
    return self.head
end

function builder_mt:Offset()
   return self.bytes.size - self.head
end

function builder_mt:Pad(n)
    if n > 0 then
        -- pads are 8-bit, so skip the bytewidth lookup
        local h = self.head - n  -- UInt8
        self.head = h
        self.bytes:Pad(n, h)
    end
end

function builder_mt:Prep(size, additionalBytes)
    if size > self.minalign then
        self.minalign = size
    end

    local h = self.head

    local k = self.bytes.size - h + additionalBytes
    local alignsize = getAlignSize(k, size)

    local desiredSize = alignsize + size + additionalBytes

    while self.head < desiredSize do
        local oldBufSize = self.bytes.size
        growByteBuffer(self, desiredSize)
        local updatedHead = self.head + self.bytes.size - oldBufSize
        self.head = updatedHead
    end

    self:Pad(alignsize)
end

function builder_mt:PrependSOffsetTRelative(off)
    self:Prep(4, 0)
    assert(off <= self:Offset(), "Offset arithmetic error")
    local off2 = self:Offset() - off + 4
    self:Place(off2, SOffsetT)
end

function builder_mt:PrependUOffsetTRelative(off)
    self:Prep(4, 0)
    local soffset = self:Offset()
    if off <= soffset then
        local off2 = soffset - off + 4
        self:Place(off2, UOffsetT)
    else
        error("Offset arithmetic error")
    end
end

function builder_mt:StartVector(elemSize, numElements, alignment)
    assert(not self.nested)
    self.nested = true
    local elementSize = elemSize * numElements
    self:Prep(4, elementSize) -- Uint32 length
    self:Prep(alignment, elementSize)
    return self:Offset()
end

function builder_mt:EndVector(vectorNumElements)
    assert(self.nested)
    self.nested = false
    self:Place(vectorNumElements, UOffsetT)
    return self:Offset()
end

function builder_mt:CreateString(s)
    assert(not self.nested)
    self.nested = true

    assert(type(s) == "string")

    self:Prep(4, #s + 1)
    self:Place(0, Uint8)

    local l = #s
    self.head = self.head - l

    self.bytes:Set(s, self.head, self.head + l)

    return self:EndVector(l)
end

function builder_mt:CreateByteVector(x)
    assert(not self.nested)
    self.nested = true

    local l = #x
    self:Prep(4, l)

    self.head = self.head - l

    self.bytes:Set(x, self.head, self.head + l)

    return self:EndVector(l)
end

function builder_mt:Slot(slotnum)
    assert(self.nested)
    -- n.b. slot number is 0-based
    self.currentVTable[slotnum + 1] = self:Offset()
end

local function finish(self, rootTable, sizePrefix)
    UOffsetT:EnforceNumber(rootTable)
    self:Prep(self.minalign, sizePrefix and 8 or 4)
    self:PrependUOffsetTRelative(rootTable)
    if sizePrefix then
        local size = self.bytes.size - self.head
        Int32:EnforceNumber(size)
        self:PrependInt32(size)
    end
    self.finished = true
    return self.head
end

function builder_mt:Finish(rootTable)
    return finish(self, rootTable, false)
end

function builder_mt:FinishSizePrefixed(rootTable)
    return finish(self, rootTable, true)
end

function builder_mt:Prepend(flags, off)
    self:Prep(flags.bytewidth, 0)
    self:Place(off, flags)
end

function builder_mt:PrependSlot(flags, o, x, d)
    flags:EnforceNumbers(x,d)
--    flags:EnforceNumber(x)
--    flags:EnforceNumber(d)
    if x ~= d then
        self:Prepend(flags, x)
        self:Slot(o)
    end
end

function builder_mt:PrependBoolSlot(...)    self:PrependSlot(Bool, ...) end
function builder_mt:PrependByteSlot(...)    self:PrependSlot(Uint8, ...) end
function builder_mt:PrependUint8Slot(...)   self:PrependSlot(Uint8, ...) end
function builder_mt:PrependUint16Slot(...)  self:PrependSlot(Uint16, ...) end
function builder_mt:PrependUint32Slot(...)  self:PrependSlot(Uint32, ...) end
function builder_mt:PrependUint64Slot(...)  self:PrependSlot(Uint64, ...) end
function builder_mt:PrependInt8Slot(...)    self:PrependSlot(Int8, ...) end
function builder_mt:PrependInt16Slot(...)   self:PrependSlot(Int16, ...) end
function builder_mt:PrependInt32Slot(...)   self:PrependSlot(Int32, ...) end
function builder_mt:PrependInt64Slot(...)   self:PrependSlot(Int64, ...) end
function builder_mt:PrependFloat32Slot(...) self:PrependSlot(Float32, ...) end
function builder_mt:PrependFloat64Slot(...) self:PrependSlot(Float64, ...) end

function builder_mt:PrependUOffsetTRelativeSlot(o,x,d)
    if x~=d then
        self:PrependUOffsetTRelative(x)
        self:Slot(o)
    end
end

function builder_mt:PrependStructSlot(v,x,d)
    UOffsetT:EnforceNumber(d)
    if x~=d then
        UOffsetT:EnforceNumber(x)
        assert(x == self:Offset(), "Tried to write a Struct at an Offset that is different from the current Offset of the Builder.")
        self:Slot(v)
    end
end

function builder_mt:PrependBool(x)      self:Prepend(Bool, x) end
function builder_mt:PrependByte(x)      self:Prepend(Uint8, x) end
function builder_mt:PrependUint8(x)     self:Prepend(Uint8, x) end
function builder_mt:PrependUint16(x)    self:Prepend(Uint16, x) end
function builder_mt:PrependUint32(x)    self:Prepend(Uint32, x) end
function builder_mt:PrependUint64(x)    self:Prepend(Uint64, x) end
function builder_mt:PrependInt8(x)      self:Prepend(Int8, x) end
function builder_mt:PrependInt16(x)     self:Prepend(Int16, x) end
function builder_mt:PrependInt32(x)     self:Prepend(Int32, x) end
function builder_mt:PrependInt64(x)     self:Prepend(Int64, x) end
function builder_mt:PrependFloat32(x)   self:Prepend(Float32, x) end
function builder_mt:PrependFloat64(x)   self:Prepend(Float64, x) end
function builder_mt:PrependVOffsetT(x)  self:Prepend(VOffsetT, x) end

function builder_mt:Place(x, flags)
    local d = flags:EnforceNumberAndPack(x)
    local h = self.head - flags.bytewidth
    self.head = h
    self.bytes:Set(d, h)
end


--------------------------- CORE MODULE -----------------------
local flatbuffers_module = {}
flatbuffers_module.Builder = builder_module.New
flatbuffers_module.N = num_types_module
flatbuffers_module.view = view_module
flatbuffers_module.binaryArray = binary_array_module

return flatbuffers_module