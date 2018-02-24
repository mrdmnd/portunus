[![Build Status](https://travis-ci.org/mrdmnd/policygen.svg?branch=master)](https://travis-ci.org/mrdmnd/policygen)
# PolicyGen

PolicyGen is a tool for discovering optimal character control policies in World
of Warcraft. We compute an optimal control policy through the use of simulation
and deep reinforcement learning.

## Background

See [Background](BACKGROUND.md) for the motivation behind PolicyGen.

## Installation

Policygen is comprised of a few major components
1) a distributed, CPU-powered RPC simulation service, for evaluating
policy networks against a minimal simulation engine (sort of like SIMC).
The idea is to scale this horizontally, with as many copies of the simulation
server running as you like. Simulation requests are sent to a load balancer
which distributes load among the service backends.
Currently, I've only implemented the "localhost" version of this.

2) a local, GPU-powered learning module that trains a model with deep learning
to evaluate the Q-function, and perform policy gradient descent.

As I'm a Google engineer in my $DAYJOB, most of the building blocks for this
project are Google open-source libraries. Let's install these now.
We assume you're on a modern x64 machine running some flavor of linux, ideally
with an NVIDIA GPU. I'm running on Arch Linux; you may need to translate these
instructions to fit your distro + package manager.


### Get Started
Let's get some dependencies built.

- [Bazel](https://bazel.build/) - you need this to build both TensorFlow and PolicyGen.
  On my system, this is installed with `sudo pacman -S bazel`.

- (OPTIONAL) [CUDA](https://developer.nvidia.com/cuda-downloads) and [cuDNN](https://developer.nvidia.com/cuDNN)
  Install these if you want CUDA support. Which you do. Duh.
  On my system, this is installed with `sudo pacman -S nvidia cuda cudnn`.

- (OPTIONAL) [Intel MKL](https://software.intel.com/en-us/mkl) - this is a
  useful tool for accelerating linear algebra if you're not planning on using a
  GPU. It might even be helpful for other things. Who knows?! You want this.
  On my system, this is installed with `yaourt -S intel-mkl`.

- [Tensorflow](https://github.com/tensorflow/tensorflow) - If you want maximum
  performance, compile this from source and run the configuration wizard. Here's
  how to do that:

  1) Install pip, numpy, and wheel: `sudo pacman -S python-pip python-numpy python-wheel`.
     If you're on a distro that doesn't ship headers, you may also need python-dev.
     If python3 isn't your default, you may want the python3 versions of these.
  2) Clone the TF repo: `git clone https://github.com/tensorflor/tensorflow`
  3) Run the configuration script: `cd tensorflow && ./configure`
     You want to point the script to your install of CUDA.

     Tell the script to target the latest version of CUDA.
     (I had to change mine from a default of 9.0 to my installed version of 9.1).

     Tell the script to compile for the compute capability of your hardware
     device (you can see this by running the deviceQuery example in the CUDA
     extras/demo_suite directory).

     Tell the script to compile for your cuDNN version.
     Accept the defaults for the other options.

  4) Run the following command, which builds a PIP package-generator for TF.
     This will take a while, and a fuckload of RAM. Go get coffee. Maybe lunch.

     ```
     bazel build --config=opt --config=cuda --config=mkl --config=monolithic //tensorflow/tools/pip_package:build_pip_package
     # INFO: Elapsed time: 1498.937s, Critical Path: 175.17s
     # INFO: Build completed successfully, 7238 total actions
     ```
  5) Run the pip-package generator we just built:

     `./bazel-bin/tensorflow/tools/pip_package/build_pip_package /tmp/tensorflow_pkg`

  6) Install the pip package from `/tmp/tensorflow_pkg`. You'll have to replace
     the version number here:

     `sudo pip install /tmp/tensorflow_pkg/tensorflow-{VERSION}.whl

  7) Verify the installation:

```
[mrdmnd@maximumkappa policygen]$ python
Python 3.6.4 (default, Jan  5 2018, 02:35:40) 
[GCC 7.2.1 20171224] on linux
Type "help", "copyright", "credits" or "license" for more information.
>>> import tensorflow as tf
>>> hello = tf.constant("Hello!")
>>> session = tf.Session()
2018-02-23 22:40:24.084161: I tensorflow/core/common_runtime/gpu/gpu_device.cc:1344] Found device 0 with properties: 
name: TITAN X (Pascal) major: 6 minor: 1 memoryClockRate(GHz): 1.531
pciBusID: 0000:01:00.0
totalMemory: 11.90GiB freeMemory: 11.25GiB
2018-02-23 22:40:24.300276: I tensorflow/core/common_runtime/gpu/gpu_device.cc:1344] Found device 1 with properties: 
name: TITAN X (Pascal) major: 6 minor: 1 memoryClockRate(GHz): 1.531
pciBusID: 0000:02:00.0
totalMemory: 11.91GiB freeMemory: 11.75GiB
2018-02-23 22:40:24.303063: I tensorflow/core/common_runtime/gpu/gpu_device.cc:1423] Adding visible gpu devices: 0, 1
2018-02-23 22:40:24.866764: I tensorflow/core/common_runtime/gpu/gpu_device.cc:911] Device interconnect StreamExecutor with strength 1 edge matrix:
2018-02-23 22:40:24.866793: I tensorflow/core/common_runtime/gpu/gpu_device.cc:917]      0 1 
2018-02-23 22:40:24.866799: I tensorflow/core/common_runtime/gpu/gpu_device.cc:930] 0:   N Y 
2018-02-23 22:40:24.866802: I tensorflow/core/common_runtime/gpu/gpu_device.cc:930] 1:   Y N 
2018-02-23 22:40:24.869499: I tensorflow/core/common_runtime/gpu/gpu_device.cc:1041] Created TensorFlow device (/job:localhost/replica:0/task:0/device:GPU:0 with 10895 MB memory) -> physical GPU (device: 0, name: TITAN X (Pascal), pci bus id: 0000:01:00.0, compute capability: 6.1)
2018-02-23 22:40:24.976689: I tensorflow/core/common_runtime/gpu/gpu_device.cc:1041] Created TensorFlow device (/job:localhost/replica:0/task:0/device:GPU:1 with 11380 MB memory) -> physical GPU (device: 1, name: TITAN X (Pascal), pci bus id: 0000:02:00.0, compute capability: 6.1)
>>> print(session.run(hello))
b'Hello!'
```

  On Arch, you might also be able to install the package 
  [tensorflow-opt-cuda](https://www.archlinux.org/packages/community/x86_64/tensorflow-opt-cuda/)
  directly.

- C++17 compliant compiler (code built + tested with clang 5.0.1 on Arch Linux)

### Developers

Optional Dependencies for developers:
- [ClangFormat](https://clang.llvm.org/docs/ClangFormat.html) - all code
  is autoformatted to Google's style guide conventions.
- [Buildifier](https://github.com/bazelbuild/buildtools) - BUILD file is
  autoformatted to Google's style guide convensions.
- [Bazel CompilationDatabase](https://github.com/grailbio/bazel-compilation-database)
  to build Compilation Database for CLANG tooling. After bazel build, invoke
  `tools/bazel-compilation-database/generate.sh` to build compilation database.
  This seeds the rest of the useful clang completion tools.

## Compile

Policygen is built with `bazel`, which is Google's opensource version of their
internal build tool. Build artifacts are compiled into `./bazel-genfiles/` and
binaries are placed into `./bazel-bin/`.

TODO(mrdmnd) - revist these targets.

To compile the simulation service, run

`bazel build --config=opt simulate:simulation_service`

To compile the learning module, run

`bazel build --config=opt learn:policygen`.

## Execute

To run the simulation service, invoke

`./bazel-bin/simulate/simulation_service`.

This starts up an async RPC service running on "localhost:50051"

To run the policy generation tool, invoke

`./bazel-bin/learn/policygen --config=$CONF`,

where the $CONF file contains details on the raid encounter as well as available
player gear.
