local _, Portunus = ...
print("You've loaded the portunus exporter addon.")
local LibDeflate = LibStub:GetLibrary("LibDeflate")


-- TODO: set some flag if the user mouses over the frame that the data may be corrupt!!

local function tprint (tbl, indent)
  if not indent then indent = 0 end
  local toprint = string.rep(" ", indent) .. "{\r\n"
  indent = indent + 2 
  for k, v in pairs(tbl) do
    toprint = toprint .. string.rep(" ", indent)
    if (type(k) == "number") then
      toprint = toprint .. "[" .. k .. "] = "
    elseif (type(k) == "string") then
      toprint = toprint  .. k ..  "= "   
    end
    if (type(v) == "number") then
      toprint = toprint .. v .. ",\r\n"
    elseif (type(v) == "string") then
      toprint = toprint .. "\"" .. v .. "\",\r\n"
    elseif (type(v) == "table") then
      toprint = toprint .. tprint(v, indent + 2) .. ",\r\n"
    else
      toprint = toprint .. "\"" .. tostring(v) .. "\",\r\n"
    end
  end
  toprint = toprint .. string.rep(" ", indent-2) .. "}"
  print(toprint)
end

-- NOTE: 127x127 seems to hit some hard limits around 128^2 pixels. You can do 128x126 also.
-- on my machine we cap out at ~16160 pixels after some exhaustive /reload binary searching xD
-- this factors nicely as 101x160.
-- we should try NOT to hit this limit as it crashes the UI whenever you mouseover something;
-- instead try something like 33x331 for now, which gives us exactly 32kib + 1 (metadata, num_bytes) of transfer.
-- works out kind of nicely.
-- if we need more storage we can ramp up towards probably a max of 16160 pixel textures.
local frameWidth = 33
local frameHeight = 331
--local frameWidth = 100
--local frameHeight = 100

Portunus.Timer = nil
Portunus.UpdatePeriod = 0.20
Portunus.CompressionEnabled = true
local max_storable_bytes = frameWidth * frameHeight * 3

local portunusPixelFrames = {}

local stringbyte = string.byte
local mathfloor = math.floor

local function InitializePixels()
    for y = 0, frameHeight - 1 do
        for x = 0, frameWidth - 1 do
            local pixel = Portunus.MainFrame:CreateTexture(nil, "BACKGROUND")
            pixel:SetSize(1, 1)
            pixel:SetPoint("TOPLEFT", x, -y)
            pixel:SetColorTexture(0, 0, 0, 1)
            portunusPixelFrames[#portunusPixelFrames+1] = pixel
        end
    end
end

-- Length of bytes must be a multiple of three here.
-- We store the "compression enabled" flag in r.
-- First pixel stores the number of bytes to decode in our screenshot reader: num_bytes -> g + 256*b
local function UpdatePixels(bytes)
    local num_bytes = #bytes
    print("Rendering " .. tostring(num_bytes) .. " bytes of data.")
    tprint(bytes)
    if num_bytes > max_storable_bytes then print("ERROR: attempted to serialize " .. tostring(num_bytes) .. " bytes but max capacity is " .. tostring(max_storable_bytes)) return end
    local pixel_index_max = num_bytes / 3
    local r = Portunus.CompressionEnabled and 1.0 or 0.0
    local g = (num_bytes % 256) / 255.0
    local b = (mathfloor(num_bytes / 256) % 256) / 255.0
    portunusPixelFrames[1]:SetColorTexture(r, g, b, 1)
    for pixel_index = 0, pixel_index_max-1 do
        r = bytes[3*pixel_index+1] / 255.0
        g = bytes[3*pixel_index+2] / 255.0
        b = bytes[3*pixel_index+3] / 255.0
        portunusPixelFrames[pixel_index+2]:SetColorTexture(r, g, b, 1) -- the plus two here is because we're offset by one for lua starting at one and then offset by one again because of the first pixel metadata
    end
end


Portunus.MainFrame = CreateFrame("Frame", "PortunusExporter_MainFrame", UIParent)
Portunus.MainFrame:SetSize(frameWidth, frameHeight)
Portunus.MainFrame:SetFrameStrata("TOOLTIP")
Portunus.MainFrame:SetPoint("TOPLEFT", 0, 0)
Portunus.MainFrame:RegisterEvent("ADDON_LOADED")

-- Uses MessagePack to encode a table as an aligned sequence of bytes with length a multiple of three.
local function MessagepackEncodeTable(table)
    local raw_message = Portunus.MessagePack.pack(table)
    -- The majority of the time spent is spent in this compression function call; do we really need compression here?
    local serialized_message = Portunus.CompressionEnabled and LibDeflate:CompressZlib(raw_message) or raw_message
    --local ratio = (#raw_message) / (#serialized_message)
    --print("Compression ratio: ", ratio)
    local bytes = {stringbyte(serialized_message, 1, #serialized_message)}
    local padding = (3 - (#bytes % 3)) % 3
    for i = 1, padding do
        bytes[#bytes + 1] = 0
    end
    return bytes
end

local function TimerCallback()
    local test_table = {
        Player = {
            HP = 100, Mana = 25
        },
        Player2 = {
            HP = 105, Mana = 25
        },
        Player3 = {
            HP = 99, Mana = 13
        },
        --AttackMode = "do it",
        --CurrentTime = GetTime()
    }
    local bytes = MessagepackEncodeTable(test_table)
    UpdatePixels(bytes)
end

Portunus.MainFrame:SetScript("OnEvent", function (self, Event, Arg1)
    if Event == "ADDON_LOADED" and Arg1 == "PortunusExporter" then
		Portunus.MainFrame:Show()
        InitializePixels()
        Portunus.Timer = C_Timer.NewTicker(Portunus.UpdatePeriod, TimerCallback)
        C_Timer.After(2, function() Portunus.MainFrame:UnregisterEvent("ADDON_LOADED") end)
    end
end)