-- This sub code was programmatically added by update_flatbuffers.py
-- It is intended to replace the `require` functionality missing from the WOW lua environment.
-- We wrap the entire module in an function called "export_fn()" and then load that fn into Portunus.Modules at the bottom of this file.
local _, Portunus = ...
local function require(m) local e=Portunus.Modules[m] if e==nil then error("Failed to load module " .. m) end return e end
local function export_fn()

--[[ game_state_schema.Resource

  Automatically generated by the FlatBuffers compiler, do not modify.
  Or modify. I'm a message, not a cop.

  flatc version: 24.3.25

  Declared by  : 
  Rooting type : game_state_schema.GameState ()

--]]

local flatbuffers = require('flatbuffers')

local Resource = {}
local mt = {}

function Resource.New()
  local o = {}
  setmetatable(o, {__index = mt})
  return o
end

function mt:Init(buf, pos)
  self.view = flatbuffers.view.New(buf, pos)
end

function mt:ResourceType()
  return self.view:Get(flatbuffers.N.Int8, self.view.pos + 0)
end

function mt:Current()
  return self.view:Get(flatbuffers.N.Uint8, self.view.pos + 1)
end

function mt:Maximum()
  return self.view:Get(flatbuffers.N.Uint8, self.view.pos + 2)
end

function Resource.CreateResource(builder, resourceType, current, maximum)
  builder:Prep(1, 3)
  builder:PrependUint8(maximum)
  builder:PrependUint8(current)
  builder:PrependInt8(resourceType)
  return builder:Offset()
end

return Resource
end
-- The above `end` keyword and the following line are designed to replace the `require` functionality missing from the WOW lua environment.
Portunus.Modules["game_state_schema.Resource"]=export_fn()
