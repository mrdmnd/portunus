#pragma once

#include "simulator/core/event.h"

#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"
#include "proto/policy.pb.h"
#include "proto/simulation.pb.h"

namespace simulator {
namespace core {

// This class is intended to hold "single encounter" configuration details.
// Specifically, we hold raid events, player config, and a policy summary.
class ConfigSummary {
 public:
  ConfigSummary() = delete;
  ConfigSummary(const simulatorproto::SimulationConfig& sim_proto);

  ConfigSummary(const ConfigSummary& other) = default;
  ConfigSummary(ConfigSummary&& other) = default;
  ConfigSummary& operator=(const ConfigSummary& other) = default;
  ConfigSummary& operator=(ConfigSummary&& other) = default;

  inline milliseconds GetTimeTarget() const { return time_target_; }
  inline int GetTimeVariance() const { return time_variance_; }
  inline std::vector<Event> GetRaidEvents() const { return raid_events_; };

 private:
  // Encounter members
  const milliseconds time_target_;
  const double time_variance_;
  const std::vector<Event> raid_events_;

  // Player members

  // Policy members
};
}  // namespace core
}  // namespace simulator
