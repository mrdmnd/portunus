load("@policygen_py_deps//:requirements.bzl", "requirement")

# This binary trains a policy net for a given (encounter conf, player conf).
py_binary(
    name = "policygen",
    srcs = [
        "policygen.py",
    ],
    deps = [
        ":simc_addon_parser",
        "//proto:py_lib",
        requirement("absl-py"),
        requirement("grpcio"),
    ],
)

# This library is meant to read data from the SIMC addon's text dump format
# and generate the various protobufs that our program needs.
py_library(
    name = "simc_addon_parser",
    srcs = ["simc_addon_parser.py"],
    deps = ["//proto:py_lib"],
)
