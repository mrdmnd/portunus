#pragma once
#include <chrono>

// A *TRIGGER* is a condition that can automatically cause a SPELL or an EFFECT
// to fire.
// Example types
// SpellHit
//
//
class Trigger {
 public:
  virtual void Predicate() = 0;
};

// This trigger fires when either a specific spell (whitelisted) or any spell
// (not blacklisted) has a successful cast.
class CastComplete : Trigger {};

class SpellHit : Trigger {
 public:
  std::chrono::milliseconds icd;
  double rppm;
  double percent_chance;
  bool exclude_items_and_enchants;
  bool exclude_auto_attacks;

  std::vector<Spell> whitelist;
  std::vector<Spell> blacklist;
};

class SpellCrit : Trigger {
 public:
  std::chrono::milliseconds icd;
  double rppm;
  double percent_chance;
  bool exclude_items_and_enchants;
  bool exclude_auto_attacks;

  std::vector<Spell> whitelist;
  std::vector<Spell> blacklist;
};

class PowerCapped : Trigger {};
