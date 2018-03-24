#pragma once
#include <chrono>

#include "simulator/core/constants.h"

namespace simulator {
namespace core {

// This struct is responsible for holding combat stats for a player.
struct CombatStats {
  CombatStats() = default;

  // Enable move + copy constructors and assignments.
  CombatStats(const CombatStats& other) = default;
  CombatStats(CombatStats&& other) = default;
  CombatStats& operator=(const CombatStats& other) = default;
  CombatStats& operator=(CombatStats&& other) = default;

  int strength;
  int agility;
  int intelligence;
  int stamina;
  int crit_rating;
  int mastery_rating;
  int versatility_rating;
  int haste_rating;
  int attack_power;
  int spell_power;

  std::chrono::milliseconds mh_speed;
  int mh_avg_damage;

  std::chrono::milliseconds oh_speed;
  int oh_avg_damage;

  // Treated as StatExtra on AMR
  std::chrono::milliseconds attack_speed;

  // See colossus smash.
  double ignore_armor_percentage;

  double damage_multiplier = 1.0;
  // Total multiplier on critical strike damage. In most cases this multiplier
  // gets applied to the final crit damage (which is 2x normal damage).
  double crit_damage_multiplier = 1.0;
  // Multiplier on extra critical damage. These combine differently than
  // regular critical damage multipliers, and the tooltips generally show half
  // of the actual value used.
  double extra_crit_damage_multiplier = 1.0;
  double strength_multiplier = 1.0;
  double agility_multiplier = 1.0;
  double intelligence_multiplier = 1.0;
  double crit_multiplier = 1.0;
  double mastery_multiplier = 1.0;
  double versatility_multiplier = 1.0;
  double haste_multiplier = 1.0;
  double attack_power_multiplier = 1.0;
  double attack_speed_multiplier = 1.0;
  double spell_power_multiplier = 1.0;
  double spell_speed_multiplier = 1.0;

  inline double TotalStrength() const { return strength * strength_multiplier; }
  inline double TotalAgility() const { return agility * agility_multiplier; }
  inline double TotalIntelligence() const {
    return intelligence * intelligence_multiplier;
  }

  inline double TotalAttackPower(const bool agil_spec) const {
    // TODO(MRDMND) - tanks get 1% AP per 1% mastery...
    double value = agil_spec ? attack_power + TotalAgility()
                             : attack_power + TotalStrength();
    return value * attack_power_multiplier;
  }

  inline double TotalSpellPower(const bool uses_attack_power) {
    // Feral druids, guardian  druids, prot pallys, ret pallys, and enhancement
    // shaman use AP as base
    return 0;
  }

  // Total attack speed bonus (this is different than haste, some buffs like SND
  // only increase attack speed) TODO(MRDMND) - fix me, use correct speed!
  inline double TotalAttackSpeed() const {
    return (1 + mh_speed.count() / 1000) * attack_speed_multiplier - 1;
  }

  inline double TotalDamageMultiplier() const {
    return damage_multiplier * (1 + TotalVersatilityPercent());
  }

  inline double TotalCritPercent() const {
    return crit_rating / simulator::core::constants::kCritRatingPerPercent;
  }
  inline double TotalMasteryPercent() const {
    return 8 + (mastery_rating /
                simulator::core::constants::kMasteryRatingPerPercent);
  }
  inline double TotalVersatilityPercent() const {
    return versatility_rating /
           simulator::core::constants::kVersatilityRatingPerPercent;
  }
  inline double TotalHastePercent() const {
    return (1 +
            haste_rating / simulator::core::constants::kHasteRatingPerPercent) *
               haste_multiplier -
           1;
  }

  /*
  inline double WeaponDamage(const bool offhand, const bool normalized,
                             const bool physical) const {
    const int damage = offhand ? oh_avg_damage : mh_avg_damage;
    const std::chrono::milliseconds speed = offhand ? oh_speed : mh_speed;
    // TODO(MRDMND) - handle normalized(3.3s 2H, 2.4s 1H,  2.8s range, 1.7s dag)
    double result =
        (damage + TotalAttackPower() / 3.5 * speed) * damage_multiplier;
    if (offhand) {
      result *= 0.5 * TotaOffhandDamageMultiplier();
    }
    if (physical) {
      result *= TargetArmorReduction();
    }
  }*/
};
}  // namespace core
}  // namespace simulator
