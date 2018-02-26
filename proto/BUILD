package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "cc_proto_library")
load("@org_pubref_rules_protobuf//python:rules.bzl", "py_proto_library")

filegroup(
    name = "protos",
    srcs = glob(["*.proto"]),
)

cc_proto_library(
    name = "cc_lib",
    protos = [":protos"],
    with_grpc = True,
)

py_proto_library(
    name = "py_lib",
    protos = [":protos"],
    with_grpc = True,
)
