--- ================= HEADER =================
--- Elemental Shaman Spell and Aura Definitions
--- ======== LOCALIZE =========
local addonName, SYN = ...

-- Create the SpellDefinitions module
SYN.ElementalSpells = SYN.ElementalSpells or {}
local Spells = SYN.ElementalSpells

--- ================ SPELL DEFINITIONS ================

-- Direct Damage Spells
Spells.LightningBolt = SYN.Spell({
    id = 188196,
    name = "Lightning Bolt",
    icon = 136048,
    minRange = 0,
    maxRange = 40,
    cooldown = 0,
    cost = 0,
    castTime = 2.0,
    gcd = 1.5,
    school = "Nature",
    category = "Damage",
    tags = {"SingleTarget", "Filler"}
})

Spells.LavaBurst = SYN.Spell({
    id = 51505,
    name = "Lava Burst",
    icon = 237582,
    minRange = 0,
    maxRange = 40,
    cooldown = 8.0,
    cost = 0,
    castTime = 2.0,
    gcd = 1.5,
    school = "Fire",
    category = "Damage",
    tags = {"SingleTarget", "Guaranteed Crit"}
})

Spells.FlameShock = SYN.Spell({
    id = 188389,
    name = "Flame Shock",
    icon = 135813,
    minRange = 0,
    maxRange = 40,
    cooldown = 6.0,
    cost = 0,
    castTime = 0,
    gcd = 1.5,
    school = "Fire",
    category = "Damage",
    tags = {"SingleTarget", "DoT", "Instant"}
})

Spells.EarthShock = SYN.Spell({
    id = 8042,
    name = "Earth Shock",
    icon = 136026,
    minRange = 0,
    maxRange = 40,
    cooldown = 0,
    cost = 60,
    costType = "Maelstrom",
    castTime = 0,
    gcd = 1.5,
    school = "Nature",
    category = "Damage",
    tags = {"SingleTarget", "Instant", "Spender"}
})

Spells.Earthquake = SYN.Spell({
    id = 61882,
    name = "Earthquake",
    icon = 451165,
    minRange = 0,
    maxRange = 40,
    cooldown = 0,
    cost = 60,
    costType = "Maelstrom",
    castTime = 0,
    gcd = 1.5,
    school = "Nature",
    category = "Damage",
    tags = {"AoE", "Instant", "Spender"}
})

Spells.ChainLightning = SYN.Spell({
    id = 188443,
    name = "Chain Lightning",
    icon = 136015,
    minRange = 0,
    maxRange = 40,
    cooldown = 0,
    cost = 0,
    castTime = 2.0,
    gcd = 1.5,
    school = "Nature",
    category = "Damage",
    tags = {"AoE", "Chain"}
})

Spells.Icefury = SYN.Spell({
    id = 210714,
    name = "Icefury",
    icon = 135855,
    minRange = 0,
    maxRange = 40,
    cooldown = 30.0,
    cost = 0,
    castTime = 2.0,
    gcd = 1.5,
    school = "Frost",
    category = "Damage",
    tags = {"SingleTarget", "Talent"}
})

Spells.FrostShock = SYN.Spell({
    id = 196840,
    name = "Frost Shock",
    icon = 135849,
    minRange = 0,
    maxRange = 40,
    cooldown = 0,
    cost = 0,
    castTime = 0,
    gcd = 1.5,
    school = "Frost",
    category = "Damage",
    tags = {"SingleTarget", "Instant", "Slow"}
})

-- Cooldowns
Spells.FireElemental = SYN.Spell({
    id = 198067,
    name = "Fire Elemental",
    icon = 135790,
    minRange = 0,
    maxRange = 0,
    cooldown = 150.0,
    cost = 0,
    castTime = 0,
    gcd = 1.5,
    school = "Fire",
    category = "Cooldown",
    tags = {"Major Cooldown", "Pet", "Instant"}
})

Spells.StormElemental = SYN.Spell({
    id = 192249,
    name = "Storm Elemental",
    icon = 2065626,
    minRange = 0,
    maxRange = 0,
    cooldown = 150.0,
    cost = 0,
    castTime = 0,
    gcd = 1.5,
    school = "Nature",
    category = "Cooldown",
    tags = {"Major Cooldown", "Pet", "Instant", "Talent"}
})

Spells.Stormkeeper = SYN.Spell({
    id = 191634,
    name = "Stormkeeper",
    icon = 839977,
    minRange = 0,
    maxRange = 0,
    cooldown = 60.0,
    charges = 1,
    cost = 0,
    castTime = 0,
    gcd = 1.5,
    school = "Nature",
    category = "Cooldown",
    tags = {"Instant", "Buff", "Talent"}
})

Spells.LiquidMagmaTotem = SYN.Spell({
    id = 192222,
    name = "Liquid Magma Totem",
    icon = 971079,
    minRange = 0,
    maxRange = 40,
    cooldown = 60.0,
    cost = 0,
    castTime = 0,
    gcd = 1.5,
    school = "Fire",
    category = "Cooldown",
    tags = {"AoE", "Totem", "Instant", "Talent"}
})

Spells.Ascendance = SYN.Spell({
    id = 114050,
    name = "Ascendance",
    icon = 135791,
    minRange = 0,
    maxRange = 0,
    cooldown = 180.0,
    cost = 0,
    castTime = 0,
    gcd = 1.5,
    school = "Nature",
    category = "Cooldown",
    tags = {"Major Cooldown", "Instant", "Transform", "Talent"}
})

-- Utility
Spells.WindShear = SYN.Spell({
    id = 57994,
    name = "Wind Shear",
    icon = 136018,
    minRange = 0,
    maxRange = 30,
    cooldown = 12.0,
    cost = 0,
    castTime = 0,
    gcd = 0, -- Off GCD
    school = "Nature",
    category = "Utility",
    tags = {"Interrupt", "Instant"}
})

Spells.Spiritwalker = SYN.Spell({
    id = 79206,
    name = "Spiritwalker's Grace",
    icon = 451169,
    minRange = 0,
    maxRange = 0,
    cooldown = 120.0,
    cost = 0,
    castTime = 0,
    gcd = 0, -- Off GCD
    school = "Nature",
    category = "Utility",
    tags = {"Mobility", "Instant"}
})

--- ================ AURA DEFINITIONS ================

SYN.ElementalAuras = SYN.ElementalAuras or {}
local Auras = SYN.ElementalAuras

-- Debuffs
Auras.FlameShock = SYN.Aura({
    id = 188389,
    name = "Flame Shock",
    icon = 135813,
    duration = 18.0,
    maxStacks = 1,
    type = "debuff",
    pandemic = true,
    pandemicWindow = 0.3, -- 30% pandemic window (5.4 seconds)
    dispelType = "Magic",
    school = "Fire",
    category = "Damage",
    priority = 100, -- High priority to maintain
    tags = {"DoT", "SingleTarget"}
})

Auras.LightningRod = SYN.Aura({
    id = 197209,
    name = "Lightning Rod",
    icon = 252934,
    duration = 8.0,
    maxStacks = 1,
    type = "debuff",
    pandemic = false, -- No pandemic, applied by talent
    dispelType = "Magic",
    school = "Nature",
    category = "Damage",
    priority = 80,
    tags = {"Talent", "Proc"}
})

-- Buffs
Auras.Stormkeeper = SYN.Aura({
    id = 191634,
    name = "Stormkeeper",
    icon = 839977,
    duration = 15.0,
    maxStacks = 2,
    type = "buff",
    pandemic = false,
    school = "Nature",
    category = "Damage",
    priority = 90,
    tags = {"Talent", "Cooldown Buff"}
})

Auras.SurgeOfPower = SYN.Aura({
    id = 285514,
    name = "Surge of Power",
    icon = 2065621,
    duration = 15.0,
    maxStacks = 1,
    type = "buff",
    pandemic = false,
    school = "Nature",
    category = "Damage",
    priority = 85,
    tags = {"Talent", "Proc"}
})

Auras.MasterOfTheElements = SYN.Aura({
    id = 260734,
    name = "Master of the Elements",
    icon = 136027,
    duration = 15.0,
    maxStacks = 1,
    type = "buff",
    pandemic = false,
    school = "Nature",
    category = "Damage",
    priority = 75,
    tags = {"Talent", "Proc"}
})

Auras.LavaSurge = SYN.Aura({
    id = 77762,
    name = "Lava Surge",
    icon = 451169,
    duration = 10.0,
    maxStacks = 1,
    type = "buff",
    pandemic = false,
    school = "Fire",
    category = "Damage",
    priority = 95,
    tags = {"Proc"}
})

Auras.Icefury = SYN.Aura({
    id = 210714,
    name = "Icefury",
    icon = 135855,
    duration = 15.0,
    maxStacks = 4,
    type = "buff",
    pandemic = false,
    school = "Frost",
    category = "Damage",
    priority = 70,
    tags = {"Talent", "Cooldown Buff"}
})

Auras.Ascendance = SYN.Aura({
    id = 114050,
    name = "Ascendance",
    icon = 135791,
    duration = 15.0,
    maxStacks = 1,
    type = "buff",
    pandemic = false,
    school = "Nature",
    category = "Cooldown",
    priority = 100,
    tags = {"Major Cooldown", "Transform", "Talent"}
})

Auras.SpiritwalkerGrace = SYN.Aura({
    id = 79206,
    name = "Spiritwalker's Grace",
    icon = 451169,
    duration = 15.0,
    maxStacks = 1,
    type = "buff",
    pandemic = false,
    school = "Nature",
    category = "Utility",
    priority = 60,
    tags = {"Mobility"}
})

--- ================ HELPER FUNCTIONS ================

-- Get spell by ID
function SYN.GetElementalSpell(spellID)
    for name, spell in pairs(Spells) do
        if spell:GetID() == spellID then
            return spell
        end
    end
    return nil
end

-- Get aura by ID
function SYN.GetElementalAura(auraID)
    for name, aura in pairs(Auras) do
        if aura:GetID() == auraID then
            return aura
        end
    end
    return nil
end

-- Get all spells with a specific tag
function SYN.GetElementalSpellsByTag(tag)
    local results = {}
    for name, spell in pairs(Spells) do
        if spell:HasTag(tag) then
            table.insert(results, spell)
        end
    end
    return results
end

-- Get all auras with a specific tag
function SYN.GetElementalAurasByTag(tag)
    local results = {}
    for name, aura in pairs(Auras) do
        if aura:HasTag(tag) then
            table.insert(results, aura)
        end
    end
    return results
end

