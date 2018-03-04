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
    name = "com_google_googletest",
    strip_prefix = "googletest-master",
    url = "https://github.com/google/googletest/archive/master.zip",
)

http_archive(
    name = "com_google_glog",
    strip_prefix = "glog-master",
    url = "https://github.com/google/glog/archive/master.zip",
)

new_http_archive(
    name = "com_google_benchmark",
    build_file = "@com_github_grpc_grpc//third_party:benchmark.BUILD",
    strip_prefix = "benchmark-master",
    url = "https://github.com/google/benchmark/archive/master.zip",
)

new http_archive(
    name = "tbb",
    build_file = "tbb.BUILD",
    url = "https://www.threadingbuildingblocks.org/sites/default/files/software_releases/source/tbb2018_20171205oss_src.tgz",
    strip_prefix = "tbb2018_20171205",
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

# We load all python dependencies in a two-phase process:
pip_repositories()

pip_import(
    name = "policygen_py_deps",
    requirements = "//learn:requirements.txt",
)

load("@policygen_py_deps//:requirements.bzl", "pip_install")

pip_install()

load("@org_pubref_rules_protobuf//cpp:rules.bzl", "cpp_proto_repositories")

cpp_proto_repositories()

load("@org_pubref_rules_protobuf//python:rules.bzl", "py_proto_repositories")

py_proto_repositories()
