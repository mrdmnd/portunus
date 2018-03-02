package(default_visibility = ["//visibility:public"])

# This is a standalone program that provides a `main` which takes a path to a
# text protobuf object representing the simulation config to execute.
cc_binary(
    name = "standalone",
    srcs = [
        "standalone.cc",
    ],
    deps = [
        ":engine",
        "//proto:cc_lib",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_absl//absl/memory",
        "@com_google_glog//:glog",
    ],
)

cc_library(
    name = "engine",
    srcs = [
        "engine.cc",
        "online_statistics.cc",
    ],
    hdrs = [
        "engine.h",
        "online_statistics.h",
    ],
    deps = [
        "//proto:cc_lib",
        "@com_google_absl//absl/strings",
        "@com_google_glog//:glog",
    ],
)

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
