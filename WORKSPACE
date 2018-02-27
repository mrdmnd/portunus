workspace(name = "com_mrdmnd_policygen")

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
    name = "io_abseil_py",
    strip_prefix = "abseil-py-master",
    url = "https://github.com/abseil/abseil-py/archive/master.zip",
)

http_archive(
    name = "com_google_googletest",
    strip_prefix = "googletest-master",
    url = "https://github.com/google/googletest/archive/master.zip",
)

http_archive(
    name = "org_tensorflow",
    strip_prefix = "tensorflow-master",
    url = "https://github.com/tensorflow/tensorflow/archive/master.zip",
)

http_archive(
    name = "com_google_glog",
    strip_prefix = "glog-master",
    url = "https://github.com/google/glog/archive/master.zip",
)

new_http_archive(
    name = "com_github_google_benchmark",
    build_file = "@com_github_grpc_grpc//third_party:benchmark.BUILD",
    strip_prefix = "benchmark-master",
    url = "https://github.com/google/benchmark/archive/master.zip",
)

# Need these two because the bazel compiler doesn't have a cc_proto_library that
# explicitly supports GRPC services yet.
# See https://github.com/pubref/rules_protobuf
http_archive(
    name = "org_pubref_rules_protobuf",
    strip_prefix = "rules_protobuf-master",
    url = "https://github.com/pubref/rules_protobuf/archive/master.zip",
)

# See https://github.com/bazelbuild/rules_python
http_archive(
    name = "io_bazel_rules_python",
    strip_prefix = "rules_python-master",
    url = "https://github.com/bazelbuild/rules_python/archive/master.zip",
)

load("@io_bazel_rules_python//python:pip.bzl", "pip_repositories", "pip_import")

pip_repositories()

pip_import(
    name = "pip_grpcio",
    requirements = "@org_pubref_rules_protobuf//python:requirements.txt",
)

load(
    "@pip_grpcio//:requirements.bzl",
    pip_grpcio_install = "pip_install",
)

pip_grpcio_install()

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "cpp_proto_repositories")

cpp_proto_repositories()

load("@org_pubref_rules_protobuf//python:rules.bzl", "py_proto_repositories")

py_proto_repositories()
