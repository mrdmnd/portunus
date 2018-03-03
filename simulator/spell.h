// This classs provides a means for defining player spells.
// A spell has a static component describing properties of "all such" spells,
// and a non-static component describing properties pertinent to this
// instantiation of the spell.

#include <string>

class Spell {
 public:
  virtual bool Execute() const = 0;

 protected:
  // GENERAL (static)
  // The common english name, i.e. "BladeFlurry"
  static const std::string name_;

  // The spell id, if present in wowhead.
  static const int id_;

  // An enum, like "Physical", "Shadow", "Frost", etc.
  static const SpellSchool school_;

  // A function provided by inheritors that returns True if the function is
  // available (checks talents, class/spec, etc).
  static const std::function<bool()> requirement_validator_fn_;

  // BEHAVIOR (static)
  // Spells have cooldowns (given here in MS)
  static const int cooldown_;
  //
  // A list of SpellEffects that this spell triggers.
  // Effects might be "Buff", "Damage", etc.
  static const std::vector<SpellEffect> effects_

      // Instances of spells have source actors and target actors.
      const Actor* source_;
  const Actor* target_;
};

class BladeFlurry : Spell {}
