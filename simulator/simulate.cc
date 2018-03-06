#include <atomic>
#include <random>

#include "glog/logging.h"

#include "proto/simulation.pb.h"

#include "simulator/config_processors.h"
#include "simulator/online_statistics.h"

#include "simulator/simulate.h"

namespace policygen {
using namespace configprocess;
void RunBatch(const EncounterSummary& encounter,
              const EquipmentSummary& equipment,
              const PolicyFunctor& policy,
              const int num_iterations,
              const std::atomic_bool& cancellation_token,
              policygen::OnlineStatistics* damage_tracker) {
  int current_iteration = 0;
  while (!cancellation_token && current_iteration < num_iterations) {
    damage_tracker->AddValue(RunSingleIteration(encounter, equipment, policy));
    ++current_iteration;
  }
}

double RunSingleIteration(const EncounterSummary& encounter,
                          const EquipmentSummary& equipment,
                          const PolicyFunctor& policy) {
  std::random_device r;
  std::default_random_engine e(r());
  std::normal_distribution<double> normal_dist(20000.0, 500.0);
  return normal_dist(e);
}

}  // namespace policygen
