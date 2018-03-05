#pragma once

#include "proto/simulation.pb.h"

// The entry point and main logic for doing a single fight iteration.
namespace policygen {
double SingleIteration(const simulatorproto::SimulationConfig& config);
}  // namespace policygen
