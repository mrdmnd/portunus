# this is an attempt at a bazel build file
# with skylark rules to build tbb with its own makefile
# then use the prebuilt library as an external dependency!
licenses(["notice"])

# make this target dependent, let bazel find them
tbb_bazel_release_build_dir="build_release"
tbb_bazel_debug_build_dir="build_debug"
genrule(
    name = 'build_tbb',
    # if available I want this built with crosstools
    # I want the binaries to be picked up and placed in the appropriate directory
    # this picks the build machines default gcc. useless
    # tbb seems to hardcode CONLY=gcc and CPLUS=g++ if selecting linux.gcc.inc
    # hence the PATH trick
    cmd = 'cd external/tbb; \
           export PATH=$$(dirname $(AR)):$$PATH; \
           NM=$(NM) AR=$(AR) make tbb_build_prefix="build" ; \
           cd ../../; \
           echo cp external/tbb/build/build_{release,debug}/*.so.2 $(@D) ; \
                cp external/tbb/build/build_{release,debug}/*.so.2 $(@D) ; \
    ',
    outs = [
        "libtbb.so.2", 
        "libtbbmalloc.so.2",
        "libtbbmalloc_proxy.so.2",
        "libtbb_debug.so.2", 
        "libtbbmalloc_debug.so.2",
        "libtbbmalloc_proxy_debug.so.2",
    ],
)

cc_library(
    name = "tbb",
    hdrs = glob([
        "include/serial/*.h",
        "include/tbb/**/*.h",
        ]),
    srcs = [ "libtbb.so.2"],
    includes = ["include"],
    visibility = ["//visibility:public"],
)
