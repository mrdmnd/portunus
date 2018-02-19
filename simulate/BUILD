package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "GRPC_COMPILE_DEPS")

################################################################################
# Common Targets

################################################################################
# Simulation Service
cc_binary(
    name = "simulation_service",
    srcs = ["simulation_service.cc"],
    deps = ["//simulate/proto:simulation_service_proto_lib"] + GRPC_COMPILE_DEPS,
)

cc_binary(
    name = "simulation_service_client",
    srcs = ["simulation_service_client.cc"],
    deps = ["//simulate/proto:simulation_service_proto_lib"] + GRPC_COMPILE_DEPS,
)

################################################################################
# Worker

################################################################################
# Tests
