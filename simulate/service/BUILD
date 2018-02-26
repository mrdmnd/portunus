package(default_visibility = ["//visibility:public"])

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "GRPC_COMPILE_DEPS")

cc_binary(
    name = "simulation_service",
    srcs = ["simulation_service.cc"],
    deps = [
        "//proto:cc_lib",
        "//simulate/engine:engine",
        "@com_github_gflags_gflags//:gflags",
        "@com_google_absl//absl/strings",
        "@com_google_absl//absl/memory",
        "@com_google_glog//:glog",
    ] + GRPC_COMPILE_DEPS,
)
