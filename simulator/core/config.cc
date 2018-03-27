#include "simulator/core/config.h"
#include "simulator/core/combat_stats.h"
#include "simulator/core/constants.h"
#include "simulator/core/event.h"

#include "proto/encounter_config.pb.h"
#include "proto/simulation.pb.h"

using std::chrono::milliseconds;

using simulator::core::enums::EventTag;

namespace simulator {
namespace core {

// Put additional RaidEvent builder definitions here.
Event BuildSpawnEvent(const simulatorproto::EncounterEvent& proto) {
  std::function<void(SimulationState*)> cb = [&proto](SimulationState* s) {
    const auto enemy_proto = proto.spawn().enemy();
    const auto name = enemy_proto.name();
    const auto est = enemy_proto.health_estimator();

    if (est == simulatorproto::HealthEstimator::UNIFORM) {
      s->enemies.push_back(std::make_unique<Enemy>(
          name, HealthEstimator::UniformHealthEstimator()));
    } else if (est == simulatorproto::HealthEstimator::BURST) {
      s->enemies.push_back(std::make_unique<Enemy>(
          name, HealthEstimator::BurstHealthEstimator()));
    } else if (est == simulatorproto::HealthEstimator::EXECUTE) {
      s->enemies.push_back(std::make_unique<Enemy>(
          name, HealthEstimator::ExecuteHealthEstimator()));
    } else if (est == simulatorproto::HealthEstimator::BURST_AND_EXECUTE) {
      s->enemies.push_back(std::make_unique<Enemy>(
          name, HealthEstimator::BurstExecuteHealthEstimator()));
    } else {
      LOG(INFO) << "Health estimator " << est << " not found.";
      LOG(INFO) << "Proceeding with BurstAndExecute default.";
      s->enemies.push_back(std::make_unique<Enemy>(
          name, HealthEstimator::BurstExecuteHealthEstimator()));
    }
  };
  return Event(milliseconds(proto.timestamp()), cb, EventTag::ENEMY_SPAWN);
}

// From an encounter proto, get a vector of raid events.
std::vector<Event> ParseRaidEvents(
    const simulatorproto::EncounterConfig& encounter_proto) {
  size_t n_events = encounter_proto.events_size();
  std::vector<Event> events;
  events.reserve(n_events);

  for (size_t i = 0; i < n_events; ++i) {
    const auto& event_proto = encounter_proto.events(i);
    switch (event_proto.event_case()) {
      case simulatorproto::EncounterEvent::kSpawn: {
        events.push_back(BuildSpawnEvent(event_proto));
        break;
      }
      case simulatorproto::EncounterEvent::kMovement: {
        break;
      }
      case simulatorproto::EncounterEvent::kLust: {
        break;
      }
      case simulatorproto::EncounterEvent::kStun: {
        break;
      }
      case simulatorproto::EncounterEvent::kDamage: {
        break;
      }
      case simulatorproto::EncounterEvent::kInvuln: {
        break;
      }
      default: { LOG(INFO) << "Attempting to add an unknown raid event."; }
    }
  }
  return events;
}

CombatStats ParseGearStats(const simulatorproto::PlayerConfig& player_proto) {
  CombatStats s;
  return s;
}

std::vector<Spell> ParseGearEffects(
    const simulatorproto::PlayerConfig& player_proto) {
  return {};
}

std::vector<Talent> ParseTalents(
    const simulatorproto::PlayerConfig& player_proto) {
  return {};
}

std::unique_ptr<PolicyInterface> ParsePolicy(
    const simulatorproto::Policy& policy_proto) {
  return std::make_unique<DeterministicPolicy>(10);
}

Config::Config(const simulatorproto::SimulationConfig& sim_proto) :
  time_min_(sim_proto.encounter_config().min_time_millis()),
  time_max_(sim_proto.encounter_config().max_time_millis()),
  raid_events_(ParseRaidEvents(sim_proto.encounter_config())),
  gear_stats_(ParseGearStats(sim_proto.player_config())),
  gear_effects_(ParseGearEffects(sim_proto.player_config())),
  talents_(ParseTalents(sim_proto.player_config())),
  policy_(ParsePolicy(sim_proto.policy())) {
  LOG(INFO) << "Constructing configuration.";
}
}  // namespace core
}  // namespace simulator
