-- This sub code was programmatically added by update_flatbuffers.py
local _, Portunus = ...
local function require(m) local e=Portunus.Modules[m] if e==nil then error("Failed to load module " .. m) end return e end
local function export_fn()

--[[ game_state_schema.PlayerUnit

  Automatically generated by the FlatBuffers compiler, do not modify.
  Or modify. I'm a message, not a cop.

  flatc version: 24.3.25

  Declared by  : 
  Rooting type : game_state_schema.GameState ()

--]]

local __game_state_schema_UnitBaseInfo = require('game_state_schema.UnitBaseInfo')
local flatbuffers = require('flatbuffers')

local PlayerUnit = {}
local mt = {}

function PlayerUnit.New()
  local o = {}
  setmetatable(o, {__index = mt})
  return o
end

function mt:Init(buf, pos)
  self.view = flatbuffers.view.New(buf, pos)
end

function mt:BaseInfo()
  local o = self.view:Offset(4)
  if o ~= 0 then
    local x = self.view:Indirect(self.view.pos + o)
    local obj = __game_state_schema_UnitBaseInfo.New()
    obj:Init(self.view.bytes, x)
    return obj
  end
end

function PlayerUnit.Start(builder)
  builder:StartObject(1)
end

function PlayerUnit.AddBaseInfo(builder, baseInfo)
  builder:PrependStructSlot(0, baseInfo, 0)
end

function PlayerUnit.End(builder)
  return builder:EndObject()
end

return PlayerUnit
end
Portunus.Modules["game_state_schema.PlayerUnit"]=export_fn()
