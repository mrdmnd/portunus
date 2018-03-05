#pragma once

#include <array>
#include <map>
#include <vector>

#include "proto/simulation.pb.h"

// This file includes World-of-Warcraft specific constant values.
// At some point we'll pull this out of the DBC files.
namespace policygen {

using namespace simulatorproto;
// Subtract one because we don't count UNUSED_SPECIALIZATION.
namespace constants {
static constexpr int kNumSpecs = simulatorproto::Specialization_ARRAYSIZE - 1;
/*
{
  "DeathKnightFrostDamage": "1.5",
  "DeathKnightUnholyShadow": "2.25",
  "DemonHunterHavocChaos": "1.5",
  "DemonHunterHavocSpeed": "1",
  "DruidBalanceDamage": "2.25",
  "DruidFeralBleed": "2",
  "HunterBeastMasteryPet": "2.25",
  "HunterMarksmanshipRange": "0.625",
  "HunterMarksmanshipDamage": "2.5",
  "HunterSurvivalChance": "0.5",
  "MageArcaneMana": "1.2",
  "MageArcaneDamage": "0.6",
  "MageFireDamage": "0.75",
  "MageFrostIcicles": "2.25",
  "MageFrostWaterElemental": "2.25",
  "MonkWindwalkerDamage": "1.25",
  "PaladinRetributionHolyPower": "1.75",
  "PaladinRetributionJudgement": "4.375",
  "PriestShadowDamage": "2.5",
  "RogueAssassinationDamage": "4",
  "RogueOutlawChance": "2.2",
  "RogueSubtletyDamage": "2.76",
  "ShamanElementalChance": "1.875",
  "ShamanEnhancementDamage": "2",
  "ShamanEnhancementChance": "0.08",
  "WarlockAfflictionDamage": "3.125",
  "WarlockDemonologyDamage": "1.8",
  "WarlockDestructionDamage": "2",
  "WarlockDestructionReduce": "0.666",
  "WarriorArmsColossusSmash": "1.6",
  "WarriorArmsDamage": "1.6",
  "WarriorFuryDamage": "1.4",
}
*/
static const std::map<Specialization, std::vector<double>> kMasteryCoeffs = {
    {Specialization::DEATH_KNIGHT_FROST, {1.5}},
    {Specialization::DEATH_KNIGHT_UNHOLY, {2.25}},
    {Specialization::DEMON_HUNTER_HAVOC, {1.5, 1.0}},
    {Specialization::DRUID_BALANCE, {2.25}},
    {Specialization::DRUID_FERAL, {2.0}},
    {Specialization::HUNTER_BEAST_MASTERY, {2.25}},
    {Specialization::HUNTER_MARKSMANSHIP, {0.625, 2.5}},
    {Specialization::HUNTER_SURVIVAL, {0.5}},
    {Specialization::MAGE_ARCANE, {1.2, 0.6}},
    {Specialization::MAGE_FIRE, {0.75}},
    {Specialization::MAGE_FROST, {2.25, 2.25}},
    {Specialization::MONK_WINDWALKER, {1.25}},
    {Specialization::PALADIN_RETRIBUTION, {1.75, 4.375}},
    {Specialization::PRIEST_SHADOW, {2.5}},
    {Specialization::ROGUE_ASSASSINATION, {4.0}},
    {Specialization::ROGUE_OUTLAW, {2.2}},
    {Specialization::ROGUE_SUBTLETLY, {2.76}},
    {Specialization::SHAMAN_ELEMENTAL, {1.875}},
    {Specialization::SHAMAN_ENHANCEMENT, {2.0, 0.08}},
    {Specialization::WARLOCK_AFFLICTION, {3.125}},
    {Specialization::WARLOCK_DEMONOLOGY, {1.8}},
    {Specialization::WARLOCK_DESTRUCTION, {2.0, 0.666}},
    {Specialization::WARRIOR_ARMS, {1.6}},
    {Specialization::WARRIOR_FURY, {1.4}},
};

// Base stats @ 110 Strength = 0, Agility = 1, Stamina = 2, Intellect = 3
static const std::map<Specialization, std::array<int, 4>> kBaseStats = {
    {Specialization::DEATH_KNIGHT_FROST, {{10232, 7532, 6259, 4002}}},
    {Specialization::DEATH_KNIGHT_UNHOLY, {{10232, 7532, 6259, 4002}}},
    {Specialization::DEMON_HUNTER_HAVOC, {{8481, 9030, 6259, 5000}}},
    {Specialization::DRUID_BALANCE, {{4402, 9030, 6259, 7328}}},
    {Specialization::DRUID_FERAL, {{4402, 9030, 6259, 7328}}},
    {Specialization::HUNTER_BEAST_MASTERY, {{6231, 9030, 6259, 6006}}},
    {Specialization::HUNTER_MARKSMANSHIP, {{6231, 9030, 6259, 6006}}},
    {Specialization::HUNTER_SURVIVAL, {{6231, 9030, 6259, 6006}}},
    {Specialization::MAGE_ARCANE, {{4550, 6252, 6259, 7328}}},
    {Specialization::MAGE_FIRE, {{4550, 6252, 6259, 7328}}},
    {Specialization::MAGE_FROST, {{4550, 6252, 6259, 7328}}},
    {Specialization::MONK_WINDWALKER, {{4402, 9030, 6259, 7328}}},
    {Specialization::PALADIN_RETRIBUTION, {{10232, 3200, 6259, 7328}}},
    {Specialization::PRIEST_SHADOW, {{5929, 7504, 6259, 7328}}},
    {Specialization::ROGUE_ASSASSINATION, {{8481, 9030, 6259, 5000}}},
    {Specialization::ROGUE_OUTLAW, {{8481, 9030, 6259, 5000}}},
    {Specialization::ROGUE_SUBTLETLY, {{8481, 9030, 6259, 5000}}},
    {Specialization::SHAMAN_ELEMENTAL, {{4402, 9030, 6259, 7328}}},
    {Specialization::SHAMAN_ENHANCEMENT, {{4402, 9030, 6259, 7328}}},
    {Specialization::WARLOCK_AFFLICTION, {{3875, 6927, 6259, 7328}}},
    {Specialization::WARLOCK_DEMONOLOGY, {{3875, 6927, 6259, 7328}}},
    {Specialization::WARLOCK_DESTRUCTION, {{3875, 6927, 6259, 7328}}},
    {Specialization::WARRIOR_ARMS, {{10232, 6252, 6259, 5000}}},
    {Specialization::WARRIOR_FURY, {{10232, 6252, 6259, 5000}}},

};

// Modifiers to base stats for each race. Str, Agi, Stam, Int
static const std::map<Race, std::array<int, 4>> kRaceStatMods = {
    {Race::BLOOD_ELF, {{-3, 1, 0, 2}}},
    {Race::DRAENEI, {{1, -3, 2, 0}}},
    {Race::DWARF, {{2, -2, 1, -1}}},
    {Race::GNOME, {{-3, 1, -1, 3}}},
    {Race::GOBLIN, {{-3, 1, -1, 3}}},
    {Race::HUMAN, {{0, 0, 0, 0}}},
    {Race::NIGHT_ELF, {{-4, 4, 0, 0}}},
    {Race::ORC, {{3, -3, 1, -1}}},
    {Race::PANDAREN, {{0, -2, 2, 0}}},
    {Race::TAUREN, {{2, -2, 2, -2}}},
    {Race::TROLL, {{1, 2, 0, -3}}},
    {Race::UNDEAD, {{2, -1, 1, -2}}},
    {Race::WORGEN, {{2, 1, 0, -3}}},
    {Race::NIGHTBORNE, {{0, -1, 1, 0}}},
    {Race::HIGHMOUNTAIN_TAUREN, {{1, -2, 2, -1}}},
    {Race::VOID_ELF, {{-3, 1, 0, 2}}},
    {Race::LIGHTFORGED_DRAENEI, {{-2, 1, -1, 2}}},
};

// Health per stamina
static constexpr int kHealthPerStamina = 60;

// Base mana by player level.
// This is the amount that a non-caster class like a Prot Paladin would have.
static constexpr int kBaseManaNonCasterDPS = 220000;
static const std::map<Specialization, int> kBaseMana = {
    {Specialization::DRUID_BALANCE, 5 * kBaseManaNonCasterDPS},
    {Specialization::DRUID_FERAL, 1 * kBaseManaNonCasterDPS},
    {Specialization::MAGE_ARCANE, 5 * kBaseManaNonCasterDPS},
    {Specialization::MAGE_FIRE, 5 * kBaseManaNonCasterDPS},
    {Specialization::MAGE_FROST, 5 * kBaseManaNonCasterDPS},
    {Specialization::PALADIN_RETRIBUTION, 1 * kBaseManaNonCasterDPS},
    {Specialization::PRIEST_SHADOW, 1 * kBaseManaNonCasterDPS},
    {Specialization::SHAMAN_ELEMENTAL, 5 * kBaseManaNonCasterDPS},
    {Specialization::SHAMAN_ENHANCEMENT, 1 * kBaseManaNonCasterDPS},
    {Specialization::WARLOCK_AFFLICTION, 5 * kBaseManaNonCasterDPS},
    {Specialization::WARLOCK_DEMONOLOGY, 5 * kBaseManaNonCasterDPS},
    {Specialization::WARLOCK_DESTRUCTION, 5 * kBaseManaNonCasterDPS},
};

// Rating needed per 1% of each stat
static constexpr int kVersatilityRatingPerPercent = 475;
static constexpr int kCritRatingPerPercent = 400;
static constexpr int kMasteryRatingPerPercent = 400;
static constexpr int kHasteRatingPerPercent = 375;

// Used for calculating damage from armor. Values are lvl 110 - lvl 113 armor.
// Index should be level of attack, not target getting hit.
static constexpr int kArmorDrConstant[] = {7390, 7648, 7906, 8164};

// The amount of armor that a mage/warr type creature has, by level (110-114).
static constexpr int kCreatureArmorMageType[] = {2516, 2604, 2692, 2779};
static constexpr int kCreatureArmorWarrType[] = {3144, 3254, 3364, 3474};
}  // namespace constants
}  // namespace policygen
