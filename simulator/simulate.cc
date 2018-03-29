#include <atomic>
#include <chrono>
#include <random>

#include "simulator/core/action.h"
#include "simulator/core/config.h"
#include "simulator/core/event.h"
#include "simulator/core/player.h"
#include "simulator/core/policy.h"
#include "simulator/core/simulation_state.h"
#include "simulator/util/online_statistics.h"
#include "simulator/util/rng.h"
#include "simulator/util/timer_wheel.h"

#include "simulator/simulate.h"

using simulator::core::Config;
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

void SimulationContext::ProcessAction(const core::Action a) {
  // TODO(mrdmnd) - use visitor pattern somewhere here.
  return;
}

SimulationContext::SimulationContext(const Config& config) :
  policy_(config.GetPolicy()) {
  // First, we select a duration uniformly at random between configuration
  // bounds to seed our combat length with.
  //
  // Next, we must set up the player object from the parsed configuration.
  //
  // Once the player is in place (and safely owned by the state_ member), we can
  // take on the task of scheduling "absolute" raid events into the manager.
  // These absolute events include things like the main target spawn.
  //
  // Finally, we schedule the player's autoattack if they're a melee class.
  combat_length_ = rng_.Uniform(config.GetTimeMin(), config.GetTimeMax());

  // Initialize player object.
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
    // It is quite possible that the only choice is "WAIT", though it may be
    // worth optmizing more here.
    const core::Action action_choice = policy_->Evaluate(state_);

    // Handle this action. Might need to schedule events.
    ProcessAction(action_choice);
  }
  return damage_log_.DamagePerSecond();
}
}  // namespace simulator
