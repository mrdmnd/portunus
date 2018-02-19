workspace(name = "com_mrdmnd_policygen")
# Need this because the bazel compiler doesn't have a cc_proto_library that
# explicitly supports GRPC services yet.

http_archive(
    name = "com_github_gflags_gflags",
    strip_prefix = "gflags-master",
    url = "https://github.com/gflags/gflags/archive/master.zip",
)

http_archive(
    name = "com_google_absl",
    strip_prefix = "abseil-cpp-master",
    url = "https://github.com/abseil/abseil-cpp/archive/master.zip",
)

http_archive(
    name = "com_google_googletest",
    strip_prefix = "googletest-master",
    url = "https://github.com/google/googletest/archive/master.zip",
)

new_http_archive(
    name = "com_github_google_benchmark",
    build_file = "@com_github_grpc_grpc//third_party:benchmark.BUILD",
    strip_prefix = "benchmark-master",
    url = "https://github.com/google/benchmark/archive/master.zip",
)

http_archive(
    name = "org_pubref_rules_protobuf",
    strip_prefix = "rules_protobuf-master",
    url = "https://github.com/pubref/rules_protobuf/archive/master.zip",
)

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "cpp_proto_repositories")

cpp_proto_repositories()
