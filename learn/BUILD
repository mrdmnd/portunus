load("@pip_grpcio//:requirements.bzl", "requirement")

py_binary(
    name = "simulation_client",
    srcs = [
        "simulation_client.py",
    ],
    deps = [
        "//proto:simulation_proto_py_lib",
        requirement("grpcio"),
    ],
)
