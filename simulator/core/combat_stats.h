#pragma once
#include <chrono>

#include "simulator/core/constants.h"

namespace simulator {
namespace core {

// Characters have both combat *ratings* as well as percentages. Players have
// base stats which contribute to ratings, and equip items which give rating
// percentages. The CharacterStats class represents the stats given

struct StatRatings {
  int strength;
  int agility;
  int intelligence;
  int stamina;
  int crit;
  int mastery;
  int versatility;
  int haste;
  int attack_power;
  int spell_power;
};

struct WeaponRatings {
  std::chrono::milliseconds speed;
  int min_damage;
  int max_damage;
};

// This class is responsible for holding *current* combat stats for a player.
class CombatStats {
 public:
  CombatStats() = delete;
  explicit CombatStats(StatRatings stats, WeaponRatings main_hand,
                       WeaponRatings off_hand);

  // Enable move + copy constructors and assignments.
  CombatStats(const CombatStats& other) = default;
  CombatStats(CombatStats&& other) = default;
  CombatStats& operator=(const CombatStats& other) = default;
  CombatStats& operator=(CombatStats&& other) = default;

  inline int Strength() const { return stats_.strength; }
  inline int Agility() const { return stats_.agility; }
  inline int Intelligence() const { return stats_.intelligence; }
  inline int Stamina() const { return stats_.stamina; }
  inline int CritRating() const { return stats_.crit; }
  inline int MasteryRating() const { return stats_.mastery; }
  inline int VersatilityRating() const { return stats_.versatility; }
  inline int HasteRating() const { return stats_.haste; }
  inline int AttackPower() const { return stats_.attack_power; }
  inline int SpellPower() const { return stats_.spell_power; }

  inline double CritPercent() const {
    return CritRating() / simulator::core::constants::kCritRatingPerPercent;
  }
  inline double MasteryPercent() const {
    return MasteryRating() /
           simulator::core::constants::kMasteryRatingPerPercent;
  }
  inline double VersatilityPercent() const {
    return VersatilityRating() /
           simulator::core::constants::kVersatilityRatingPerPercent;
  }
  inline double HastePercent() const {
    return HasteRating() / simulator::core::constants::kHasteRatingPerPercent;
  }

 private:
  StatRatings stats_;
  WeaponRatings main_hand_;
  WeaponRatings off_hand_;
};
}  // namespace core
}  // namespace simulator
