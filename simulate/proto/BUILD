package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "cc_proto_library")

filegroup(
    name = "actor_proto",
    srcs = ["actor.proto"],
)

filegroup(
    name = "policy_proto",
    srcs = ["policy.proto"],
)

filegroup(
    name = "simulation_service_proto",
    srcs = ["simulation_service.proto"],
)

cc_proto_library(
    name = "actor_proto_lib",
    protos = [":actor_proto"],
)

cc_proto_library(
    name = "policy_proto_lib",
    protos = [":policy_proto"],
)

cc_proto_library(
    name = "simulation_service_proto_lib",
    proto_deps = [
        ":actor_proto_lib",
        ":policy_proto_lib",
    ],
    protos = [":simulation_service_proto"],
    with_grpc = True,
)
