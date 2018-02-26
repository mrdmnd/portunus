package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "cc_proto_library")
load("@org_pubref_rules_protobuf//python:rules.bzl", "py_proto_library")
# This is confusing: there are *two* {cc,py}_proto_library rules defined now.
# There's the core bazel implementation, and there's the pubref_rules_protobuf
# implementation that defines a "protos" field (the core has a "deps" field).
# We need the second implementation for a gRPC version. Gross.

filegroup(
    name = "all_protos",
    srcs = glob(["*.proto"]),
)

cc_proto_library(
    name = "cc_lib",
    protos = [":all_protos"],
    with_grpc = True,
)

py_proto_library(
    name = "py_lib",
    protos = [":all_protos"],
    with_grpc = True,
)
