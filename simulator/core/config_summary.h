#pragma once
#include <chrono>

#include "simulator/core/event.h"

#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"
#include "proto/policy.pb.h"
#include "proto/simulation.pb.h"

namespace simulator {
namespace core {

// This class is intended to hold "single encounter" configuration details.
// Specifically, we hold raid events, a player config, and a policy.
class ConfigSummary {
 public:
  ConfigSummary() = delete;
  ConfigSummary(const simulatorproto::SimulationConfig& sim_proto);

  ConfigSummary(const ConfigSummary& other) = default;
  ConfigSummary(ConfigSummary&& other) = default;
  ConfigSummary& operator=(const ConfigSummary& other) = default;
  ConfigSummary& operator=(ConfigSummary&& other) = default;

  inline std::chrono::milliseconds GetTimeMin() const { return time_min_; }
  inline std::chrono::milliseconds GetTimeMax() const { return time_max_; }
  inline std::vector<Event> GetRaidEvents() const { return raid_events_; }

  inline CombatStats GetGearStats() const { return gear_stats_; }
  inline std::vector<Spell> GetGearEffects() const { return gear_effects_; }
  inline std::vector<Talent> GetTalents() const { return talents_; }

  inline PolicyInterface GetPolicy() const { return policy_; }

 private:
  // Encounter members
  const std::chrono::milliseconds time_min_;
  const std::chrono::milliseconds time_max_;
  const std::vector<Event> raid_events_;

  // Player members
  const CombatStats gear_stats_;
  const std::vector<Spell> gear_effects_;
  const std::vector<Talent> talents_;

  // Policy members
  const PolicyInterface policy_;
};
}  // namespace core
}  // namespace simulator
