package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "cc_proto_library")
load("@org_pubref_rules_protobuf//python:rules.bzl", "py_proto_library")

filegroup(
    name = "protos",
    srcs = [
        "actor.proto",
        "policy.proto",
        "simulation_service.proto",
    ],
)

cc_proto_library(
    name = "simulation_proto_cc_lib",
    protos = [":protos"],
    with_grpc = True,
)

py_proto_library(
    name = "simulation_proto_py_lib",
    protos = [":protos"],
    with_grpc = True,
)
