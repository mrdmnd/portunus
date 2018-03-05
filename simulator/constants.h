#pragma once

#include <array>
#include <map>
#include <vector>

#include "proto/player_config.pb.h"

// This file includes World-of-Warcraft specific constant values.
// At some point we'll pull this out of the DBC files.
namespace policygen {
static constexpr int kNumSpecializations = 24;

static constexpr int lol = 5;
static constexpr simulatorproto::Specialization foo =
    simulatorproto::Specialization::ROGUE_ASSASSINATION;
}  // namespace policygen
