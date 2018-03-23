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
  int mh_min_damage;
  int mh_max_damage;

  std::chrono::milliseconds oh_speed;
  int oh_min_damage;
  int oh_max_damage;

  double damage_multiplier;

  inline double CritPercent() const {
    return crit_rating / simulator::core::constants::kCritRatingPerPercent;
  }
  inline double MasteryPercent() const {
    return mastery_rating /
           simulator::core::constants::kMasteryRatingPerPercent;
  }
  inline double VersatilityPercent() const {
    return versatility_rating /
           simulator::core::constants::kVersatilityRatingPerPercent;
  }
  inline double HastePercent() const {
    return haste_rating / simulator::core::constants::kHasteRatingPerPercent;
  }
};
}  // namespace core
}  // namespace simulator
