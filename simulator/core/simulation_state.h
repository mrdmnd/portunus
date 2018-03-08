#include <chrono>
#include <vector>

#include "simulator/core/enemy.h"
#include "simulator/core/player.h"

// This struct holds the current state-of-the-world for the simulator.
// It keeps track of the current time elapsed, as well as all actors.
// The idea here is to ONLY keep values that we can DIRECTLY get from the WOW
// API, so we can provide a LUA function that maps game-state directly into a
// SimulationState object. *Events*

struct SimulationState {
  std::chrono::milliseconds combat_time_elapsed;
  bool combat_potion_used;
  Player* player;
  std::vector<Enemy*> enemies;  // by convention, enemies[0] is main raid boss
}
