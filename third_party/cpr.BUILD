cc_library(
    name = "cpr",
    srcs = glob(["cpr/*.cpp"]),
    hdrs = glob(["include/cpr/*.h"]),
    copts = ["-Iexternal/whoshuu_cpr/include"],
    strip_include_prefix = "include",
    linkopts = ["-lcurl"],
    linkstatic = 1,
    visibility = ["//visibility:public"],
)
