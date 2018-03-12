#pragma once

#include <chrono>
#include <memory>
#include <vector>

#include "simulator/core/enemy.h"
#include "simulator/core/player.h"

// This struct holds the current state-of-the-world for the simulator.
// It keeps track of the current time elapsed, as well as all actors.
// The idea here is to ONLY keep values that we can DIRECTLY get from the WOW
// API, so we can provide a LUA function that maps game-state directly into a
// SimulationState object. This basically represents the "state of combat".
//
// The SimulationState object *OWNS* all actors in the simulation.
namespace simulator {
namespace core {
class SimulationState {
 public:
  bool combat_potion_used;
  std::chrono::milliseconds combat_time_elapsed;
  std::unique_ptr<Player> player;
  std::vector<std::unique_ptr<Enemy>> enemies;
};
}  // namespace core
}  // namespace simulator
