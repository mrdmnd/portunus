// This file contains the main entry point for conducting simulation.

#include <iostream>
#include <string>
#include <thread>

#include "simulate/engine/engine.h"

#include "proto/simulation.pb.h"
using namespace simulate;

SimulationResult Engine::PerformSimulation(SimulationConfig config) const {
  SimulationResult r;
  r.set_dps_mean(100.0);
  r.set_dps_variance(10.0);
  r.set_metadata("Hello world from engine.cc!");
  return r;
}
