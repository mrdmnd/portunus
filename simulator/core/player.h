#include "simulator/actor.h"
#include "simulator/constants.h"

// A collection of classes for handling "current" values for actors.
// Const or Static values (unchanging, known at simulation launch time) don't
// belong here.

namespace simulator {
namespace core {
// This class is responsible for holding the *current* combat ratings for the
// player. We may (at some point) want to implement a caching mechanism here, so
// the field access is hidden behind const accessor methods.
class CharacterStats {
 public:
  CharacterStats() = delete;
  ~CharacterStats() = default;
  explicit CharacterStats(int strength,
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

  // Accessors

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

  // Enable move + copy constructors and assignments.
  CharacterStats(const CharacterStats& other) = default;
  CharacterStats(CharacterStats&& other) = default;
  CharacterStats& operator=(const CharacterStats& other) = default;
  CharacterStats& operator=(CharacterStats&& other) = default;

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
};

// Everything needed to keep track of *CURRENT* player state for the
// instantiated player. That means variables in here probably should NOT be
// `const`. Heuristic: if you want to reference a non-time-varying value, it
// should probably be in static storage as a const-variable in the class module.
// Examples of this sort of time-invariant thing might min power, max power,
// base GCD, base power regen, etc.

// All actor state is changed through the RESOLUTION of EVENTs, and *ONLY*
// through the resolution of events. This is important. We will batch up event
// resolution so that all events that happen in a time slice are resolved
// together. This means we either need to make sure that event resolution does
// not care about events happening in that time slice (treated as simultaneous)
// or be careful about event resolution order. This is an speed vs. accuracy
// tradeoff: we can make the timing slice tighter and tighter (we will start at
// 32 windows per second, for a timing window of 31.25 milliseconds per slice)
// to improve accuracy, but this will slow down simulation.

class Player : Actor {
 public:
 private:
  // Players have character stats, but actors do not.
  CharacterStats character_stats_;

  // Players have cooldown status
  std::vector<SpellCooldown> cooldowns_;

  // Players have a special global cooldown.
  SpellCooldown gcd_;

  // Players have resource (power) - energy, mana, focus, fury, rage, runes,
  // maelstrom, astral power, etc. The idea is to pick `power` as a type with
  // ~continuous behavior, hence why runes are here and runic power is below.
  ResourceType power_;

  // Some players may have "alternate power" (mana, chi, combo_points,
  // runic_power, insanity, holy_power, soul_shards etc). Mana might be listed
  // here because some classes have mana as a resource but don't spend it as
  // their primary (spriests, warlocks, non-arcane mages, balance druids)
  ResourceType alternate_power_ = nullptr;

  // Players also have talents, but because those are fixed throughout
  // simulation, we don't persist them here.

  // Players can see all available targets to hit at all time.
  const std::vector<Enemy*> available_enemies_;
};
}  // namespace core
}  // namespace simulator
