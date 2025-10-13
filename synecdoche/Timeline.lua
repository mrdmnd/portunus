--- ================= HEADER =================
--- ======== LOCALIZE =========
--- Addon
local addonName, SYN = ...

-- File locals
local Timeline = {}
local events = {}

-- Categorization keywords (very rough heuristics)
local EVENT_CATEGORIES = {
    MOVEMENT = "movement",
	DEFENSIVE = "defensive",
	IMMUNE = "immune",
	BURST = "burst",
}

-- ======== GLOBALIZE =========
SYN.Timeline = Timeline

--- ================ HELPERS ================
local function now()
	return GetTime()
end

local function toLowerSafe(str)
	if type(str) ~= "string" then return "" end
	return string.lower(str)
end

local function purgeExpiredUnsafe(currentTime)
	local keep = {}
	for i = 1, #events do
		local e = events[i]
		if not e.expiresAt or e.expiresAt > currentTime then
			keep[#keep + 1] = e
		end
	end
	events = keep
end

local function findEventIndexById(id)
	for i = 1, #events do
		if events[i].id == id then return i end
	end
	return nil
end

local function normalizeCategory(cat)
	local lc = toLowerSafe(cat)
	if lc == EVENT_CATEGORIES.MOVEMENT then return lc end
	if lc == EVENT_CATEGORIES.DEFENSIVE then return lc end
	if lc == EVENT_CATEGORIES.BURST then return lc end
	if lc == EVENT_CATEGORIES.IMMUNE then return lc end
	-- alias
	if lc == "untargetable" then return EVENT_CATEGORIES.IMMUNE end
	return lc
end

--- ================ API ================
function Timeline:Init()
	-- No-op for now; will eventually hook into bigwigs/littlewigs
end

function Timeline:PurgeExpired()
	purgeExpiredUnsafe(now())
end

function Timeline:AddEventAt(timestamp, duration, category, label, source, id)
	local startTime = timestamp or now()
	local dur = duration or 0
	local expires = startTime + dur
	local cat = category
	local ev = {
		id = id or (tostring(source or "manual") .. ":" .. tostring(label)
			.. ":" .. tostring(startTime)),
		label = label or "",
		category = cat or "unknown",
		source = source or "manual",
		startAt = startTime,
		duration = dur,
		expiresAt = expires,
		confidence = "definite",
	}
	-- Replace existing with same id
	local idx = findEventIndexById(ev.id)
	if idx then
		events[idx] = ev
	else
		events[#events + 1] = ev
	end
	return ev.id
end

function Timeline:AddEventIn(inSeconds, duration, category, label, source, id)
	local t = now() + (inSeconds or 0)
	return self:AddEventAt(t, duration, category, label, source, id)
end

function Timeline:RemoveById(id)
	local idx = findEventIndexById(id)
	if idx then
		table.remove(events, idx)
		return true
	end
	return false
end

function Timeline:GetNextEvent(category, withinSeconds)
	self:PurgeExpired()
	local limit = withinSeconds and (now() + withinSeconds) or nil
	local best, bestTime = nil, math.huge
	for i = 1, #events do
		local e = events[i]
		if (not category or e.category == category) then
			if e.startAt < bestTime and (not limit or e.startAt <= limit) then
				best = e
				bestTime = e.startAt
			end
		end
	end
	return best
end

function Timeline:HasUpcoming(category, withinSeconds)
	return self:GetNextEvent(category, withinSeconds) ~= nil
end

function Timeline:All()
	self:PurgeExpired()
	return events
end

function Timeline:ClearAll()
	events = {}
end

--- ============ DEBUG ============
SLASH_SYNTIMELINE1 = "/syntimeline"
SlashCmdList["SYNTIMELINE"] = function(msg)
	local text = msg or ""
	local args = {}
	for token in string.gmatch(text, "%S+") do args[#args + 1] = token end
	local cmd = args[1] and string.lower(args[1]) or "list"

	if cmd == "add" then
		-- Usage: /syntimeline add <inSeconds> <duration> <category> <label...>
		if #args < 5 then
			print("Usage: /syntimeline add <in> <dur> <category> <label...>")
			return
		end
		local inSec = tonumber(args[2]) or 0
		local dur = tonumber(args[3]) or 0
		local cat = normalizeCategory(args[4])
		local label = table.concat(args, " ", 5)
		local id = Timeline:AddEventIn(inSec, dur, cat, label, "slash_command_source")
		print(string.format("Added: %s in %.1fs for %.1fs [%s] id=%s",
			label, inSec, dur, cat, id or "?"))
		return
	end

	if cmd == "clear" then
		Timeline:ClearAll()
		print("Timeline cleared.")
		return
	end

	-- default: list
	Timeline:PurgeExpired()
	print("Synecdoche timeline (", #events, ") upcoming:")
	for i = 1, #events do
		local e = events[i]
		local inSec = e.startAt - now()
		print(string.format("- %s in %.1fs for %.1fs [%s] id=%s",
			e.label or "(no label)", inSec, e.duration or 0,
			e.category or "unknown", e.id or "?"))
	end
end



