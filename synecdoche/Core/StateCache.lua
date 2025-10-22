--- ============================ HEADER ============================
--- ======= LOCALIZE =======
-- Addon
local addonName, SYN = ...

--- ======= GLOBALIZE =======
SYN.StateCache = SYN.StateCache or {}

--- ============================ CONTENTS ============================

local cache = {
	initialized = false,
	timestamp = 0,
	spec = nil,           -- { specId, className, specName }
	role = nil,           -- "TANK" | "HEALER" | "DAMAGER" | nil
	spells = {},          -- [spellName] = true
	spellIds = {},        -- [spellId] = true
	talents = {},         -- [nodeId] = true (selected talent nodes)
	talentSpellIds = {},  -- [spellId] = true (spell IDs from talents)
	heroTree = nil,       -- active hero tree/subtree ID
	gear = {},            -- [slotId] = { itemId, link }
	stats = {},           -- key/value of ratings and derived stats
}

local function now()
	return GetTime and GetTime() or 0
end

local function updateSpec()
	local currentSpec = GetSpecialization and GetSpecialization()
	if not currentSpec then
		cache.spec = nil
		cache.role = nil
		return
	end
	local specId = GetSpecializationInfo(currentSpec)
	local className, specName = nil, nil
	if specId and SYN.SpecID_ClassesSpecs and SYN.SpecID_ClassesSpecs[specId] then
		className = SYN.SpecID_ClassesSpecs[specId][1]
		specName = SYN.SpecID_ClassesSpecs[specId][2]
	end
	cache.spec = { specId = specId, className = className, specName = specName }
	if GetSpecializationRole then
		cache.role = GetSpecializationRole(currentSpec)
	end
end

local function scanSpellbook()
	local spells = {}
	local spellIds = {}
	if not GetNumSpellTabs or not GetSpellTabInfo then
		cache.spells, cache.spellIds = spells, spellIds
		return
	end
	local numTabs = GetNumSpellTabs()
	for i = 1, numTabs do
		local _, _, offset, numSpells = GetSpellTabInfo(i)
		for s = 1, numSpells do
			local slot = offset + s
			local type, id = GetSpellBookItemInfo(slot, BOOKTYPE_SPELL)
			if type == "SPELL" and id then
				local name = GetSpellBookItemName(slot, BOOKTYPE_SPELL)
				if name then
					spells[name] = true
					spellIds[id] = true
				end
			end
		end
	end
	cache.spells = spells
	cache.spellIds = spellIds
end

local INVSLOT_FIRST = 1
local INVSLOT_LAST = 19 -- WoW inventory slots typically 1..19

local function scanGear()
	local gear = {}
	for slot = INVSLOT_FIRST, INVSLOT_LAST do
		local itemId = GetInventoryItemID("player", slot)
		local link = GetInventoryItemLink("player", slot)
		if itemId or link then
			gear[slot] = { itemId = itemId, link = link }
		else
			gear[slot] = nil
		end
	end
	cache.gear = gear
end

local function scanStats()
	local stats = {}
	-- Basic combat ratings (if available)
	if GetCombatRating then
		stats.critRating = GetCombatRating(CR_CRIT_MELEE or 9)
		stats.hasteRating = GetCombatRating(CR_HASTE_MELEE or 18)
		stats.masteryRating = GetCombatRating(CR_MASTERY or 26)
		stats.versatilityRating = GetCombatRating(CR_VERSATILITY_DAMAGE_DONE or 29)
	end
	-- Derived chances (if available)
	if GetCritChance then stats.critChance = GetCritChance() end
	if GetHaste then stats.hastePercent = GetHaste() end
	if GetMasteryEffect then stats.masteryEffect = GetMasteryEffect() end
	if GetVersatilityBonus and stats.versatilityRating then
		stats.versatilityBonus = GetVersatilityBonus(stats.versatilityRating)
	end
	-- Item level
	if GetAverageItemLevel then
		local avg, equipped = GetAverageItemLevel()
		stats.avgItemLevel = avg
		stats.equippedItemLevel = equipped
	end
	cache.stats = stats
end

local function scanTalents()
	local talentSpellIds = {}
	local heroTreeId = nil
	local talentNodeIds = {}
	
	-- TWW uses C_Traits API for talents
	if not C_Traits or not C_ClassTalents then
		cache.talentSpellIds = talentSpellIds
		cache.heroTree = heroTreeId
		cache.talents = talentNodeIds
		return
	end
	
	local configId = C_ClassTalents.GetActiveConfigID()
	if not configId then
		cache.talentSpellIds = talentSpellIds
		cache.heroTree = heroTreeId
		cache.talents = talentNodeIds
		return
	end
	
	local configInfo = C_Traits.GetConfigInfo(configId)
	if not configInfo then
		cache.talentSpellIds = talentSpellIds
		cache.heroTree = heroTreeId
		cache.talents = talentNodeIds
		return
	end
	
	-- Get active hero tree/subtree
	if configInfo.activeSubTreeID then
		heroTreeId = configInfo.activeSubTreeID
	end
	
	-- Scan all talent trees (class, spec, hero)
	if configInfo.treeIDs then
		for _, treeId in ipairs(configInfo.treeIDs) do
			local nodes = C_Traits.GetTreeNodes(treeId)
			if nodes then
				for _, nodeId in ipairs(nodes) do
					local nodeInfo = C_Traits.GetNodeInfo(configId, nodeId)
					if nodeInfo and nodeInfo.currentRank and 
						nodeInfo.currentRank > 0 then
						-- Store node ID for O(1) lookup
						talentNodeIds[nodeId] = true
						
						-- Get the spell IDs from this talent
						if nodeInfo.entryIDs then
							for _, entryId in ipairs(nodeInfo.entryIDs) do
								local entryInfo = C_Traits.GetEntryInfo(
									configId, 
									entryId)
								if entryInfo and entryInfo.definitionID then
									local defInfo = 
										C_Traits.GetDefinitionInfo(
											entryInfo.definitionID)
									if defInfo and defInfo.spellID then
										talentSpellIds[defInfo.spellID] = true
									end
								end
							end
						end
					end
				end
			end
		end
	end
	
	cache.talents = talentNodeIds
	cache.talentSpellIds = talentSpellIds
	cache.heroTree = heroTreeId
end

local function refreshAll()
	updateSpec()
	scanSpellbook()
	scanTalents()
	scanGear()
	scanStats()
	cache.timestamp = now()
end

function SYN.StateCache:Get()
	return cache
end

function SYN.StateCache:Refresh()
	refreshAll()
end

-- O(1) lookup: check if a spell is known (spellbook or talent)
function SYN.StateCache:IsSpellKnown(spellId)
	return cache.spellIds[spellId] or cache.talentSpellIds[spellId]
end

-- O(1) lookup: check if a talent node is selected
function SYN.StateCache:IsTalentNodeSelected(nodeId)
	return cache.talents[nodeId] or false
end

function SYN.StateCache:Init()
	if cache.initialized then return end
	cache.initialized = true

	if not self.frame then
		self.frame = CreateFrame("Frame")
	end

	local f = self.frame

	-- Initialization events
	f:RegisterEvent("PLAYER_LOGIN")
	f:RegisterEvent("PLAYER_ENTERING_WORLD")

	-- Talents / Spec updates
	f:RegisterEvent("PLAYER_TALENT_UPDATE")
	f:RegisterEvent("PLAYER_SPECIALIZATION_CHANGED")
	f:RegisterEvent("ACTIVE_TALENT_GROUP_CHANGED")

	-- Spellbook / known spells updates
	f:RegisterEvent("SPELLS_CHANGED")

	-- Gear changes
	f:RegisterEvent("PLAYER_EQUIPMENT_CHANGED")
	f:RegisterEvent("UNIT_INVENTORY_CHANGED")

	-- Stat updates
	f:RegisterEvent("COMBAT_RATING_UPDATE")
	f:RegisterEvent("PLAYER_AVG_ITEM_LEVEL_UPDATE")
	f:RegisterEvent("UNIT_STATS")

	f:SetScript("OnEvent", function(_, event, arg1)
		if event == "PLAYER_LOGIN" or event == "PLAYER_ENTERING_WORLD" then
			refreshAll()
			return
		end

		if event == "PLAYER_TALENT_UPDATE" or event == "PLAYER_SPECIALIZATION_CHANGED" or event == "ACTIVE_TALENT_GROUP_CHANGED" then
			updateSpec()
			scanTalents()
			cache.timestamp = now()
			return
		end

		if event == "SPELLS_CHANGED" then
			scanSpellbook()
			cache.timestamp = now()
			return
		end

		if event == "PLAYER_EQUIPMENT_CHANGED" then
			-- arg1 is slotId
			local slot = arg1
			if slot then
				local itemId = GetInventoryItemID("player", slot)
				local link = GetInventoryItemLink("player", slot)
				if itemId or link then
					cache.gear[slot] = { itemId = itemId, link = link }
				else
					cache.gear[slot] = nil
				end
			else
				scanGear()
			end
			cache.timestamp = now()
			return
		end

		if event == "UNIT_INVENTORY_CHANGED" then
			if arg1 == "player" then
				scanGear()
				cache.timestamp = now()
			end
			return
		end

		if event == "UNIT_STATS" then
			if arg1 ~= "player" then return end
			scanStats()
			cache.timestamp = now()
			return
		end

		if event == "COMBAT_RATING_UPDATE" or event == "PLAYER_AVG_ITEM_LEVEL_UPDATE" then
			scanStats()
			cache.timestamp = now()
			return
		end
	end)

	-- Initial population in case events already fired
	refreshAll()
end

--- ============================ NOTE ============================
-- StateCache commands have been moved to Commands.lua
-- Use: /syn cache <action>