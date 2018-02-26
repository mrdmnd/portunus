load("@pip_grpcio//:requirements.bzl", "requirement")

py_binary(
    name = "simulation_client",
    srcs = [
        "simulation_client.py",
    ],
    deps = [
        "//proto:py_lib",
        requirement("grpcio"),
    ],
)
