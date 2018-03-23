#pragma once

#include <chrono>
#include <vector>

// A *TRIGGER* is a condition that can automatically cause a SPELL or an EFFECT
// to fire.
// Example types
// SpellHit
namespace simulator {
namespace core {

class Trigger {
 public:
  virtual void Predicate() = 0;
};

// This trigger fires when either a specific spell (whitelisted) or any spell
// (not blacklisted) has a successful cast.
class CastComplete : Trigger {};

class SpellHit : Trigger {
 public:
  // const std::chrono::milliseconds icd;
  // const double rppm;
  // const double percent_chance;
  // const bool exclude_items_and_enchants;
  // const bool exclude_auto_attacks;

  // const std::vector<Spell> whitelist;
  // const std::vector<Spell> blacklist;
};

class SpellCrit : Trigger {
 public:
  // const std::chrono::milliseconds icd;
  // const double rppm;
  // const double percent_chance;
  // const bool exclude_items_and_enchants;
  // const bool exclude_auto_attacks;
  // const std::vector<Spell> whitelist;
  // const std::vector<Spell> blacklist;
};

class PowerCapped : Trigger {};
}  // namespace core
}  // namespace simulator
