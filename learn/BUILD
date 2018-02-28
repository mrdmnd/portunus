load("@policygen_py_deps//:requirements.bzl", "requirement")

# This binary trains a policy net for a given (encounter conf, player conf).
py_binary(
    name = "policygen",
    srcs = [
        "policygen.py",
    ],
    deps = [
        ":encounter_builder",
        ":player_builder",
        "//proto:py_lib",
        requirement("absl-py"),
        requirement("grpcio"),
    ],
)

# This library provides a builder interface to do encounter configuration.
# The first concrete implementation takes data directly from a .textproto file.
# Possible other implementations include (eventually) warcraft logs files!
py_library(
    name = "encounter_builder",
    srcs = ["encounter_builder.py"],
    deps = ["//proto:py_lib"],
)

# This library provides a builder interface to do player configuration.
# The first concrement implementation takes data from the SIMC addon dump.
# Possible other implementations include the WoW armory.
py_library(
    name = "player_builder",
    srcs = ["player_builder.py"],
    deps = [
        ":gearset_summarizer",
        "//proto:py_lib",
    ],
)

# This library converts Gearset protos into EquipmentSummary protos.
py_library(
    name = "gearset_summarizer",
    srcs = ["gearset_summarizer.py"],
    deps = ["//proto:py_lib"],
)
