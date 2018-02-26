// This file contains the main entry point for conducting simulation.

#include <iostream>
#include <string>
#include <thread>

#include "simulate/engine/engine.h"

#include "proto/simulation.pb.h"
using namespace simulate;

SimulationResult Engine::Simulate(SimulationConfig config) const {
  Distribution dps_distribution;
  dps_distribution.set_mean(100.0);
  dps_distribution.set_variance(10.0);

  SimulationResult r;
  r.mutable_dps_distribution()->MergeFrom(dps_distribution);
  r.set_metadata("Hello world from engine.cc!");
  return r;
}
