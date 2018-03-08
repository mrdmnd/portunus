namespace simulator {
namespace core {

// Characters have both combat *ratings* as well as percentages. Players have
// base stats which contribute to ratings, and equip items which give rating
// percentages.

using StrengthRating int;
using AgilityRating int;
using IntelligenceRating int;
using StaminaRating int;
using CritRating int;
using MasteryRating int;
using VersatilityRating int;
using HasteRating int;
using AttackPowerRating int;
using SpellPowerRating int;

using WeaponSpeed std::chrono::milliseconds;
using WeaponDamageMin int;
using WeaponDamageMax int;

struct StatRatings {
  StrengthRating strength;
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

class CharacterStats {
 public:
  CharacterStats() = delete;
  explicit CharacterStats(CombatRatings ratings, Weapon main_hand, Weapon off_hand)
                          int agility,
                          int intelligence,
                          int stamina,
                          int crit,
                          int mastery,
                          int versatility,
                          int haste,
                          int attack_power,
                          int spell_power,
                          double mainhand_speed,
                          int mainhand_damage_min,
                          int mainhand_damage_max,
                          double offhand_speed,
                          int offhand_damage_min,
                          int offhand_damage_max) :
    strength_rating_(strength),
    agility_rating_(agility),
    intelligence_rating_(intelligence),
    stamina_rating_(stamina),
    crit_rating_(crit),
    mastery_rating_(mastery),
    versatility_rating_(versatility),
    haste_rating_(haste),
    attack_power_(attack_power),
    spell_power_(spell_power),
    mainhand_speed_(mainhand_speed),
    mainhand_damage_min_(mainhand_damage_min),
    mainhand_damage_max_(mainhand_damage_max),
    offhand_speed_(offhand_speed),
    offhand_damage_min_(offhand_damage_min),
    offhand_damage_max_(offhand_damage_max){};

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

  // Modifiers
  inline void ModifyStrengthRating(const int rating_modification_amount) {
    strength_rating_ += rating_modification_amount;
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
