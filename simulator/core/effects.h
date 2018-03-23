#pragma once

#include <chrono>
#include <optional>

#include "simulator/core/constants.h"
#include "simulator/core/simulation_state.h"
#include "simulator/core/triggers.h"

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

namespace simulator {
namespace core {

// Forward declaration to break circular dependency
// simulation_state->player->spell->effect->simulation_state
class SimulationState;
class Effect {
 public:
  virtual void Execute(SimulationState* state) = 0;

  // If specified, this effect triggers from these game events *ONLY*.
  // Any effect with a non-NONE trigger list is ONLY caused by these triggers,
  // and not executed when the parent spell is cast or triggered.
  const std::optional<std::vector<Trigger>> trigger_list;
};

class ApplyDirectDamage : Effect {
 public:
  // const TargetingInfo targeting_info;  // how many targets, etc
  // const std::function<double()> damage_computation;
  // const bool weapon_attack;
};

class ApplyPeriodicDamage : Effect {
 public:
  // const TargetingInfo targeting;
  // const std::function<double()> damage_computation;
  // const std::chrono::milliseconds duration;
  // const std::chrono::milliseconds interval;
  // const PeriodRefreshRule refresh_rule;
};

class GrantAura : Effect {
 public:
  // const std::chrono::milliseconds duration;

  // Core attributes, attack speed, power regen rate, etc
  // const simulator::core::enums::Attribute buffed_attribute;
};

class ExtendAura : Effect {
 public:
  // const std::chrono::milliseconds duration;
  // const int spell_id;  // Pointer to a buff to extend?
};

class ConsumeAura : Effect {};

class GrantResource : Effect {
 public:
  // const double resource_amount;
  // const simulator::core::enums::Resource resource;
};

class ConsumeResource : Effect {
 public:
  // const double resource_amount;
  // const simulator::core::enums::Resource resource;
};

class ModifyCooldown : Effect {
 public:
  // const std::chrono::milliseconds duration_delta;
  // const Cooldown affected_cooldown;
};

// Used for spells whose implementation requires a delay, like DeathFromAbove
class StartTimer : Effect {
 public:
  // const std::chrono::milliseconds duration;
  // const bool tick_immediately;
  // const std::optional<int> tick_count;
};

class SummonTemporaryPet : Effect {
 public:
  // const std::chrono::milliseconds duration;
};

// See Exsanguinate
class AccelerateDot : Effect {
 public:
  // const double tick_acceleration;
};

// Used to persist some information about the execution of a spell, like
// how many combo points were used on a DFA, or something like that.
class SaveValue : Effect {};
}  // namespace core
}  // namespace simulator
