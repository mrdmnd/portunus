load("@pip_grpcio//:requirements.bzl", "requirement")

py_binary(
    name = "simulation_client",
    srcs = [
        "simulation_client.py",
    ],
    deps = [
        "//proto:py_lib",
        "@io_absl_py//absl/flags",
        "@io_absl_py//absl/logging",
        requirement("grpcio"),
    ],
)

# This binary takes a collection of all of the user's gear and returns a pruned
# list of "boundary" gearsets (sets that aren't strictly dominated by others).
py_binary(
    name = "gearset_generator",
    srcs = [
        "gearset_generator.py",
    ],
    deps = [
        "//proto:py_lib",
        "@io_absl_py//absl/flags",
        "@io_absl_py//absl/logging",
    ],
)

# This binary trains a policy net for a given (encounter conf, player conf).
py_binary(
    name = "policygen",
    srcs = [
        "policygen.py",
        ],
    deps = [
        "//proto:py_lib",
        "@io_absl_py//absl/flags",
        "@io_absl_py//absl/logging",
        requirement("grpcio"),
    ],

