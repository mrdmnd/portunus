load("@pip_grpcio//:requirements.bzl", "requirement")

# This binary trains a policy net for a given (encounter conf, player conf).
py_binary(
    name = "policygen",
    srcs = [
        "policygen.py",
    ],
    deps = [
        "//proto:py_lib",
        "@io_abseil_py//absl:app",
        requirement("grpcio"),
    ],
)
