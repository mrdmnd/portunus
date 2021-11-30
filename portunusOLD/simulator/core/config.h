#pragma once
#include <chrono>

#include "simulator/core/event.h"
#include "simulator/core/policy.h"
#include "simulator/core/spell.h"
#include "simulator/core/talent.h"

#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"
#include "proto/policy.pb.h"
#include "proto/simulation.pb.h"

namespace simulator {
namespace core {

// This class is intended to hold "single encounter" configuration details.
// Specifically, we hold raid events, a player config, and a policy.
class Config {
 public:
  Config() = delete;
  Config(const simulatorproto::SimulationConfig& sim_proto);

  Config(const Config& other) = default;
  Config(Config&& other) = default;
  Config& operator=(const Config& other) = default;
  Config& operator=(Config&& other) = default;

  inline std::chrono::milliseconds GetTimeMin() const { return time_min_; }
  inline std::chrono::milliseconds GetTimeMax() const { return time_max_; }
  inline std::vector<Event> GetRaidEvents() const { return raid_events_; }

  inline CombatStats GetGearStats() const { return gear_stats_; }
  inline std::vector<Spell> GetGearEffects() const { return gear_effects_; }
  inline std::vector<Talent> GetTalents() const { return talents_; }

  // The Config object owns the PolicyInterface uniqueptr.
  // Callers get a view into the config's policy.
  inline PolicyInterface* GetPolicy() const { return policy_.get(); }

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
  std::unique_ptr<PolicyInterface> policy_;
};
}  // namespace core
}  // namespace simulator
