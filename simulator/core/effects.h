#pragma once

#include <chrono>
#include "absl/types/optional.h"

#include "simulator/core/constants.h"
#include "simulator/core/simulation_state.h"

// An *EFFECT* is the result of an *ACTION*.
// They inherit a lot of detail from their parent ACTION.
// Effects cause "pure mutations" on simulation state.
// A single ACTION may have many EFFECTS.

// Example EFFECT types:
// PeriodicDamage (represents some instance of a DOT)
// Damage (represents some instance of damage happening to a target)
// Heal
// ResourceGain
// GrantBuff (stat, damage multipliers, etc)
// ConsumeBuff (nightstalker on destealth, etc)
// StartTimer
// SummonPet

class Effect {
 public:
  virtual void Execute(SimulationState* state) = 0;

  // If specified, this effect triggers from these game events *ONLY*.
  // Any effect with a non-NONE trigger list is ONLY caused by these triggers,
  // and not executed when the parent spell is cast or triggered.
  virtual absl::optional<std::vector<Trigger>> trigger_list;
};

class ApplyDirectDamage : Effect {
 public:
  TargetingInfo targeting_info;  // how many targets, etc
  std::function<double()> damage_computation;
  bool weapon_attack;
};

class ApplyPeriodicDamage : Effect {
 public:
  TargetingInfo targeting;
  std::function<double()> damage_computation;
  std::chrono::milliseconds duration;
  std::chrono::milliseconds interval;
  PeriodRefreshRule refresh_rule;
};

class GrantAura : Effect {
 public:
  std::chrono::milliseconds duration;

  // Core attributes, attack speed, power regen rate, etc
  simulator::core::enums::Attribute buffed_attribute;
};

class ExtendAura : Effect {
 public:
  std::chrono::milliseconds duration;
  int spell_id;  // Pointer to a buff to extend?
}

class ConsumeAura : Effect {
};

class GrantResource : Effect {
 public:
  double resource_amount;
  simulator::core::enums::Resource resource;
};

class ConsumeResource : Effect {
 public:
  double resource_amount;
  simulator::core::enums::Resource resource;
};

class ModifyCooldown : Effect {
 public:
  std::chrono::milliseconds duration_delta;
  Cooldown affected_cooldown;
}

// Used for spells whose implementation requires a delay, like DeathFromAbove
class StartTimer : Effect {
 public:
  std::chrono::milliseconds duration;
  int tick_count;         // optional
  bool tick_immediately;  // optional

}

class SummonTemporaryPet : Effect {
 public:
  std::chrono::milliseconds duration;
}

// See Exsanguinate
class AccelerateDot : Effect {
 public:
  double tick_acceleration;
};

// Used to persist some information about the execution of a spell, like
// how many combo points were used on a DFA, or something like that.
class SaveValue : Effect {};
