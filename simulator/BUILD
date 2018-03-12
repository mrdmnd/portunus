package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "GRPC_COMPILE_DEPS")

############# Libraries ##############

# Simulation engine related code (launching tasks, etc).
cc_library(
    name = "engine",
    srcs = [
        "engine.cc",
        "simulate.cc",
    ],
    hdrs = [
        "engine.h",
        "simulate.h",
    ],
    deps = [
        ":core",
        ":util",
        "//proto:cc_lib",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_absl//absl/memory",
        "@com_google_absl//absl/strings",
        "@com_google_glog//:glog",
    ],
)

# Core simulation primitives like player, aura, constants, etc.
# Ideally this doesn't depend on the proto lib. Might be unavoidable.
cc_library(
    name = "core",
    srcs = glob(
        [
            "core/*.cc",
            "core/*.h",
        ],
        exclude = [
            "core/*_test.cc",
            "core/*_bench.cc",
        ],
    ),
    hdrs = glob(["core/*.h"]),
    deps = [
        "@com_google_glog//:glog",
    ],
)

# Useful bits and pieces.
cc_library(
    name = "util",
    srcs = glob(
        ["util/*.cc, util/*.h"],
        exclude = [
            "util/*_test.cc",
            "util/*_bench.cc",
        ],
    ),
    hdrs = glob(["util/*.h"]),
    deps = [
        "//proto:cc_lib",
        "@com_google_glog//:glog",
    ],
)

############# Binaries ##############

# This is a standalone program that provides a `main` which takes a path to a
# text protobuf object representing the simulation config to execute. Useful for
# one-off simulation runs.
cc_binary(
    name = "standalone",
    srcs = ["launch_standalone.cc"],
    deps = [
        ":engine",
        "//proto:cc_lib",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_absl//absl/memory",
        "@com_google_glog//:glog",
    ],
)

# This is the core simulation service that spins up a long-running engine and
# handles multiple simulation requests. Meant to be used with policygen learning
# core for rapid training of a policy model, as the policy evaluation core.
cc_binary(
    name = "service",
    srcs = ["launch_service.cc"],
    deps = [
        ":engine",
        "//proto:cc_lib",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_glog//:glog",
        "@com_google_absl//absl/strings",
        "@com_google_absl//absl/memory",
    ] + GRPC_COMPILE_DEPS,
)

############# Tests ##############

# Engine

# Core

cc_test(
    name = "config_processor_test",
    size = "small",
    srcs = ["core/config_processor_test.cc"],
    deps = [
        ":core",
        "//proto:cc_lib",
        "@com_google_googletest//:gtest_main",
    ],
)

# Util

cc_test(
    name = "threadpool_test",
    size = "small",
    srcs = ["util/threadpool_test.cc"],
    deps = [
        ":util",
        "@com_google_googletest//:gtest_main",
    ],
)

cc_test(
    name = "rng_test",
    size = "small",
    srcs = ["util/rng_test.cc"],
    deps = [
        ":util",
        "@com_google_googletest//:gtest_main",
    ],
)

cc_test(
    name = "map_interpolate_test",
    size = "small",
    srcs = ["util/map_interpolate_test.cc"],
    deps = [
        ":util",
        "@com_google_googletest//:gtest_main",
    ],
)

cc_test(
    name = "online_statistics_test",
    size = "small",
    srcs = ["util/online_statistics_test.cc"],
    deps = [
        ":util",
        "@com_google_googletest//:gtest_main",
    ],
)

cc_test(
    name = "timer_wheel_test",
    size = "small",
    srcs = ["util/timer_wheel_test.cc"],
    deps = [
        ":util",
        "@com_google_absl//absl/memory",
        "@com_google_googletest//:gtest_main",
    ],
)

############# Benchmarks ##############
cc_binary(
    name = "timer_wheel_bench",
    srcs = ["util/timer_wheel_bench.cc"],
    deps = [
        ":util",
        "@com_google_benchmark//:benchmark",
    ],
)
