local addonName, Portunus = ...
print("You've loaded the portunus exporter addon.")
local pixelSize = 2
local frameWidth = 64
local frameHeight = 64
local portunusPixelFrames = {}

local function InitializePixels()
    for x = 0, (frameWidth / pixelSize) - 1 do
        for y = 0, (frameHeight / pixelSize) - 1 do
            local r = x / (frameWidth / pixelSize)
            local g = y / (frameHeight / pixelSize)
            local b = 0
            local pixel = Portunus.MainFrame:CreateTexture(nil, "BACKGROUND")
            pixel:SetSize(pixelSize, pixelSize)
            pixel:SetPoint("TOPLEFT", x * pixelSize, -y * pixelSize)
            pixel:SetColorTexture(r, g, b, 1)
            portunusPixelFrames[#portunusPixelFrames+1] = pixel
        end
    end
end

-- Length of bytes must be a multiple of three here.
local function UpdatePixels(bytes)
    for x = 0, (frameWidth / pixelSize) - 1 do
        for y = 0, (frameHeight / pixelSize) - 1 do
            local byte_index  = 1 + 3*y + x * (frameWidth / pixelSize)
            local frame_index = 1 +   y + x * (frameWidth / pixelSize) -- lua is one-based so we have a +1 at the beginning on both of these
            if byte_index > #bytes then
                portunusPixelFrames[frame_index]:SetColorTexture(0, 0, 0, 1)
            else
                local r = bytes[byte_index+0]
                local g = bytes[byte_index+1]
                local b = bytes[byte_index+2]
                portunusPixelFrames[frame_index]:SetColorTexture(r, g, b, 1)
            end
        end
    end
end

Portunus.Timer = nil
Portunus.UpdatePeriod = 1.0
Portunus.MainFrame = CreateFrame("Frame", "PortunusExporter_MainFrame", UIParent)
Portunus.MainFrame:SetSize(frameWidth, frameHeight)
Portunus.MainFrame:SetFrameStrata("TOOLTIP")
Portunus.MainFrame:SetPoint("TOPLEFT", 0, 0)
Portunus.MainFrame:RegisterEvent("ADDON_LOADED")

local function TimerCallback()
    -- Ideally, this looks like this:
    -- local gameStateMegaObject = GetGameState()
    -- local bytes = flatbuffer.serialize(gameStateMegaObject).into_bytes

    -- This is demo data.
    local bytes = {255,   0,   0,
                     0, 255,   0,
                     0,   0, 255,}
    UpdatePixels(bytes)
end

Portunus.MainFrame:SetScript("OnEvent", function (self, Event, Arg1)
    if Event == "ADDON_LOADED" and Arg1 == "PortunusExporter" then
		Portunus.MainFrame:Show()
        InitializePixels()

        local ok = pcall(require, "jit")
        if not ok then print ("not ok") else print("ok") end
        -- prints not ok

        local ok, bit = pcall(require, "bit32")
        if not ok then print ("no bit32") else print("you got it") end
        assert(ok, "The Bit32 library must be installed")

        Portunus.Timer = C_Timer.NewTicker(Portunus.UpdatePeriod, TimerCallback)
        C_Timer.After(2, function() Portunus.MainFrame:UnregisterEvent("ADDON_LOADED") end)
    end
end)