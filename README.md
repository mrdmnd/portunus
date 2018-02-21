[![Build Status](https://travis-ci.org/mrdmnd/policygen.svg?branch=master)](https://travis-ci.org/mrdmnd/policygen)
# PolicyGen

PolicyGen is a tool for discovering optimal character control policies in World
of Warcraft. We compute an optimal control policy through the use of simulation
and deep reinforcement learning.

## Background
See [Background](BACKGROUND.md) for more information on the project.

## Installation

### Get Started

Dependencies (some are pulled in automatically)
- [Bazel](https://bazel.build/) - you must install this.
- [CUDA](https://developer.nvidia.com/cuda-downloads) - you must install this.
- [gTest](https://github.com/google/googletest) - automatic.
- [gLog](https://github.com/google/glog) - automatic.
- [gFlags](https://github.com/gflags/gflags) - automatic.
- [gRPC](https://github.com/grpc/grpc) - automatic.
- [Benchmark](https://github.com/google/benchmark) - automatic.
- [Protobuf](https://github.com/google/protobuf) - automatic.
- [Tensorflow](https://github.com/tensorflow/tensorflow)
- C++17 compliant compiler (code built + tested with clang 5.0.1 on Arch Linux)
- CUDA-supported device

### Help Develop

Optional Dependencies for developers Linters:
- [ClangFormat](https://clang.llvm.org/docs/ClangFormat.html) - all code
  is autoformatted to Google's style guide conventions.
- [Buildifier](https://github.com/bazelbuild/buildtools) - BUILD file is
  autoformatted to Google's style guide convensions.

```
git clone  https://github.com/bazelbuild/buildtools.git
cd buildtools/
bazel build //buildifier
sudo cp bazel-bin/buildifier/buildifier /usr/bin/buildifier
```

Third-party inclusions:
- [Bazel CompilationDatabase](https://github.com/grailbio/bazel-compilation-database)
  to build Compilation Database for CLANG tooling. After bazel build, invoke
  `tools/bazel-compilation-database/generate.sh` to build compilation database.
  This seeds the rest of the useful clang completion tools.



