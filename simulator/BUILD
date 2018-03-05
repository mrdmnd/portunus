package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "GRPC_COMPILE_DEPS")

# Consider hooking the compile command script a la
# https://gist.github.com/bsilver8192/0115ee5d040bb601e3b7
# and
# https://github.com/cybrown-zoox/bazel-clang-analyzer

############# Libraries ##############

cc_library(
    name = "engine",
    srcs = [
        "constants.h",
        "engine.cc",
        "online_statistics.cc",
        "simulate.cc",
        "threadpool.h",
    ],
    hdrs = [
        "engine.h",
        "online_statistics.h",
        "simulate.h",
    ],
    deps = [
        "//proto:cc_lib",
        "@com_google_absl//absl/strings",
        "@com_google_glog//:glog",
    ],
)

############# Binaries ##############

# This is a standalone program that provides a `main` which takes a path to a
# text protobuf object representing the simulation config to execute.
cc_binary(
    name = "standalone",
    srcs = ["standalone_binary.cc"],
    deps = [
        ":engine",
        "//proto:cc_lib",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_absl//absl/memory",
        "@com_google_glog//:glog",
    ],
)

# This is a simulation service that spins up a long-running engine + threadpool
# and handles multiple, long-lived simulation requests.
cc_binary(
    name = "service",
    srcs = ["service_binary.cc"],
    deps = [
        ":engine",
        "//proto:cc_lib",
        "@com_google_glog//:glog",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_absl//absl/strings",
        "@com_google_absl//absl/memory",
    ] + GRPC_COMPILE_DEPS,
)

############# Tests ##############

cc_test(
    name = "online_statistics_test",
    size = "small",
    srcs = [
        "online_statistics_test.cc",
    ],
    deps = [
        ":engine",
        "@com_google_googletest//:gtest_main",
    ],
)
