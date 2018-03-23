#pragma once

#include <chrono>
#include <optional>

#include "simulator/core/constants.h"
#include "simulator/core/effects.h"
#include "simulator/core/triggers.h"

// An *SPELL* causes one or more *EFFECTS* (see effect.h).
// Effects cause "pure mutations" on simulation state.
// A single SPELL may have multiple effects.
// SPELLs may have TRIGGERSs that automatically cause them to fire
// (e.g. Second Shuriken, which fires 30% of the time Shuriken Storm goes off)

namespace simulator {
namespace core {

class Spell {
 public:
  std::chrono::milliseconds cooldown;
  simulator::core::enums::SpellSchool school;

  std::optional<std::chrono::milliseconds> cast_time;
  std::optional<std::chrono::milliseconds> channel_time;
  std::optional<std::chrono::milliseconds> gcd_override;

  int power_cost;
  int alternate_power_cost;

  // Engine parameters
  bool active;
  bool usable_while_casting;   // Think "fire blast" - rare.
  bool triggers_gcd;           // Fairly straight-forward for on-GCD.
  bool castable_while_moving;  // Can you cast this on-the-move?

  // Behavior implementation
  // If spell triggers are specified, all Effects registered to this spell that
  // do not have their own triggers will execute when this spell executes. If
  // this is an Active spell, things that proc on CastSuccess and CastComplete
  // of this spell will also aexecte, as if this spell had been directly cast
  // (it is basically "cast" by the trigger resolving).
  // std::vector<Trigger> triggers;

  // List of effects to execute when the Spell resolves.
  // std::vector<Effect> effects;
};
}  // namespace core
}  // namespace simulator
