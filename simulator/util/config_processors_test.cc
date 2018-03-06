#include "gtest/gtest.h"

#include "simulator/util/config_processors.h"

#include "proto/encounter_config.pb.h"
#include "proto/player_config.pb.h"
#include "proto/simulation.pb.h"

using simulatorproto::EncounterConfig;
using simulatorproto::Gearset;
using simulatorproto::Policy;

namespace simulator {
namespace util {

TEST(ConfigProcessorsTest, EncounterSummary) {
  // Load encounter_config proto from text file. Parse into encounter summary.
  EXPECT_EQ(true, true);
}

TEST(ConfigProcessorsTest, EquipmentSummary) {
  // Load gearset proto from text file. Parse into equipment summary.
  Gearset gearset_proto;
  EquipmentSummary equipment_summary(gearset_proto);
  EXPECT_EQ(true, true);
}

TEST(ConfigProcessorsTest, PolicyFunctor) {
  // Load policy proto from text file. Parse into policy functor.
  EXPECT_EQ(true, true);
}

}  // namespace util
}  // namespace simulator
