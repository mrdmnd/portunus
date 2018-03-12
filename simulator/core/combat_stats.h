#pragma once

#include "simulator/core/constants.h"

namespace simulator {
namespace core {

// Characters have both combat *ratings* as well as percentages. Players have
// base stats which contribute to ratings, and equip items which give rating
// percentages. The CharacterStats class represents the stats given

struct StatRatings {
  simulator::core::enums::Attribute::STRENGTH s;
  AgilityRating agility;
  IntelligenceRating intelligence;
  StaminaRating stamina;
  CritRating crit;
  MasteryRating mastery;
  VersatilityRating versatility;
  HasteRating haste;
  AttackPowerRating attack_power;
  SpellPowerRating spell_power;
}

struct WeaponRatings {
  WeaponSpeed speed;
  WeaponDamageMin min_damage;
  WeaponDamageMax max_damage;
}

// This class is responsible for holding *current* combat stats for a player.
// We may (at some point) want to implement a caching mechanism here, so field
// access is hidden behind const accessor methods.

class Stats {
 public:
  CharacterStats() = delete;
  explicit CharacterStats(CombatRatings ratings, WeaponRatings main_hand,
                          Weapon off_hand)

      // Enable move + copy constructors and assignments.
      CharacterStats(const CharacterStats& other) = default;
  CharacterStats(CharacterStats&& other) = default;
  CharacterStats& operator=(const CharacterStats& other) = default;
  CharacterStats& operator=(CharacterStats&& other) = default;

  // Accessors
  template <typename T>
  inline int CombatStatRating<T>() const {
    return 0;
  }

  inline int StrengthRating() const { return strength_rating_; }
  inline int AgilityRating() const { return agility_rating_; }
  inline int IntelligenceRating() const { return intelligence_rating_; }
  inline int StaminaRating() const { return stamina_rating_; }
  inline int CritRating() const { return crit_rating_; }
  inline double CritPercent() const {
    return crit_rating_ / kCritRatingPerPercent;
  }
  inline int MasteryRating() const { return mastery_rating_; }
  inline double MasteryPercent() const {
    return mastery_rating_ / kMasteryRatingPerPercent;
  }
  inline int VersatilityRating() const { return versatility_rating_; }
  inline double VersatilityPercent() const {
    return versatility_rating_ / kVersatilityRatingPerPercent;
  }
  inline int HasteRating() const { return haste_rating_; }
  inline double HastePercent() const {
    return haste_rating_ / kHasteRatingPerPercent;
  }

 private:
  int strength_rating_;
  int agility_rating_;
  int intelligence_rating_;
  int stamina_rating_;
  int crit_rating_;
  int mastery_rating_;
  int versatility_rating_;
  int haste_rating_;
  int attack_power_;
  int spell_power_;
  double mainhand_speed_;
  int mainhand_damage_min_;
  int mainhand_damage_max_;
  double offhand_speed_;
  int offhand_damage_min_;
  int offhand_damage_max_;
};  // namespace core

}  // namespace core
}  // namespace simulator
