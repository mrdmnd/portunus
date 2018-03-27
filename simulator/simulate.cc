#include <atomic>
#include <chrono>
#include <random>

#include "simulator/core/config_summary.h"
#include "simulator/core/event.h"
#include "simulator/core/player.h"
#include "simulator/core/policy.h"
#include "simulator/core/simulation_state.h"
#include "simulator/util/online_statistics.h"
#include "simulator/util/rng.h"
#include "simulator/util/timer_wheel.h"

#include "simulator/simulate.h"

using simulator::core::ConfigSummary;
using simulator::core::Player;
using simulator::core::SimulationState;
using simulator::core::enums::EventTag;

using simulator::util::OnlineStatistics;
using simulator::util::RngEngine;
using simulator::util::TimerEvent;
using simulator::util::TimerWheel;

using std::chrono::milliseconds;

namespace simulator {
void SimulationContext::InitPlayer(const core::CombatStats& gear_stats,
                                   const std::vector<core::Spell>& gear_effects,
                                   const std::vector<core::Talent>& talents) {
  // Construct a player, and put them into the simulation state.
  state_.player = std::make_unique<Player>(gear_stats, gear_effects, talents);
}

// Load fixed, known-time encounter events (spawn, bloodlust, ...) into manager.
void SimulationContext::InitRaidEvents(
    const std::vector<core::Event>& raid_events) {
  for (const auto& raid_event : raid_events) {
    // Immediately execute any spawn events, or events with scheduled_time == 0
    // Don't bother scheduling them, just fire them off directly.
    if (raid_event.GetTag() == EventTag::ENEMY_SPAWN ||
        raid_event.GetScheduledTime() == std::chrono::milliseconds::zero()) {
      raid_event.GetCallback()(&state_);
    } else {
      const auto cb = raid_event.GetCallback();
      TimerEvent<std::function<void(SimulationState*)>, SimulationState*> e(
          {cb, &state_});
      event_manager_.Schedule(&e, raid_event.GetScheduledTime().count());
    }
  }
}

SimulationContext::SimulationContext(const ConfigSummary& config) :
  policy_(config.GetPolicy()) {
  // The policy_, combat_length_, rng_, state_, event_manager_, and damage_log_
  // members be initialized in this constructor. We take care of policy_ through
  // the initializer list. rng_, state_, event_manager_, and damage_log_ are all
  // default-constructible, so we rely on that. To seed combat_length, we take
  // a duration uniformly at random between configuration bounds.
  //
  // Next, we must set up the player object from the parsed configuration.
  //
  // Once the player is in place (and safely owned by the state_ member), we can
  // take on the task of scheduling "absolute" raid events into the manager.
  // These absolute events include things like the main target spawn.
  // Finally, we
  combat_length_ = rng_.Uniform(config.GetTimeMin(), config.GetTimeMax());

  // Initialize player object, store in sim state.
  InitPlayer(config.GetGearStats(), config.GetGearEffects(),
             config.GetTalents());

  // An "encounter" is actually a pre-combat phase followed by a combat phase.
  // The very first event in the "precombat" phase should be a boss spawn event.
  // Then we allow a single harmful pre-combat action. When that action impacts,
  // we mark the beginning of the "combat" phase.

  // Set up all raid events (enemy spawns, pre-combat phase, etc).
  InitRaidEvents(config.GetRaidEvents());
}

// This is our core event loop.
double SimulationContext::RunSingleIteration() {
  while (event_manager_.Now() < combat_length_.count()) {
    // While the simulation isn't done, advance until the next event is ready.
    // We process all events on that tick. These events may themselves add more
    // events to the manager (through the process of resolving triggers, etc).
    event_manager_.Advance(event_manager_.TicksUntilNextEvent());

    // Use our policy to identify the best legal action for this state.
    // const core::Spell action = policy_->Evaluate(state_);

    // Schedule this action using the event manager.
    // ProcessAction(action, state_);
  }
  return damage_log_.DamagePerSecond();
}
}  // namespace simulator
