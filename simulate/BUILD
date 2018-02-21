package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "GRPC_COMPILE_DEPS")

################################################################################
# Common Targets

################################################################################
# Simulation Service
cc_binary(
    name = "simulation_service",
    srcs = ["simulation_service.cc"],
    deps = [
        "//simulate/proto:simulation_service_proto_lib",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_absl//absl/strings",
    ] + GRPC_COMPILE_DEPS,
)

cc_binary(
    name = "simulation_client",
    srcs = ["simulation_client.cc"],
    deps = [
        "//simulate/proto:simulation_service_proto_lib",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_absl//absl/strings",
    ] + GRPC_COMPILE_DEPS,
)

################################################################################
# Worker

################################################################################
# Tests
