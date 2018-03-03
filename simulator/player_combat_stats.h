namespace policygen {
// This class is responsible for holding the *current* combat ratings for the
// player. We may (at some point) want to implement a caching mechanism here, so
// the field access is currently hidden behind const accessor methods.

class PlayerCombatStats {
 public:
  PlayerCombatStats() = delete;
  ~PlayerCombatStats() = default;
  PlayerCombatStats(int strength,
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
                    int offhand_damage_max)
      : strength_(strength);

  inline int StrengthRating() { return strength_rating_ }
  const;

  inline int AgilityRating() { return agility_rating_ }
  const;

  inline int IntelligenceRating() { return intelligence_rating_ }
  const;

  inline int StaminaRating() { return stamina_rating_ }
  const;

  inline int CritRating() { return crit_rating_ }
  const;

  inline int MasteryRating() { return mastery_rating_ }
  const;

  inline int VersatilityRating() { return versatility_rating_ }
  const;

  inline int HasteRating() { return haste_rating_ }
  const;

  // Enable move + copy constructors and assignments.
  Gearset(const Gearset& other) = default;
  Gearset(const Gearset&& other) = default;
  Gearset& operator=(const Gearset& other) = default;
  Gearset& operator=(const Gearset&& other) = default;

 private:
  int strength_rating_;
  int agility_rating;
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
}  // namespace policygen
