[![Build Status](https://travis-ci.org/mrdmnd/policygen.svg?branch=master)](https://travis-ci.org/mrdmnd/policygen)
# PolicyGen

PolicyGen is a tool for automatically finding optimal character control policies
in World of Warcraft via simulation and deep reinforcement learning.

## Background

See [Background](BACKGROUND.md) for the motivation behind PolicyGen.

## Installation

Policygen contains two core components:

1) A networked, distributed, CPU-powered RPC simulation service, for evaluating
policy functions against a minimal simulation engine (sort of like SIMC-lite).
This component scales horizontally, intended to be run behind a load balancer in
a cloud computing environment.

2) A local GPU-powered reinforcement learning module (built on TensorFlow) that
trains a deep neural network to evaluate a (state, action) Q-function, and
performs policy gradient descent to iteratively improve our policy.

Policygen depends heavily on [Bazel](https://bazel.build/) to build itself and
its dependencies, most of which are Google-backed open-source libraries.
Let's install some of these now. We assume a modern 64-bit architecture running
some flavor of linux with a C++11 compiler, ideally with an NVIDIA GPU. The code
is tested on Arch Linux; you may need to modify these instructions to fit your
distribution or package manager.

## Explicit anti-goals

In order to streamline the implementation of our simulation engine, we
explicitly do not support characters below maximum level, and characters that
are not using a DPS specialization. Policygen does not plan to support items 
implemented prior to the current expansion.

### Requirements

Policygen requires a modern compiler toolchain that supports C++11. In addition,
if you want to leverage the GPU acceleration of TensorFlow's NN training, you
must have access to a CUDA enabled video card. Finally, we assume a Python3
installation. At the time of writing, Policygen does not explicitly intend to
support Windows. Any Linux or OSX based machines should have no problems.

- [Bazel](https://bazel.build/) - you'll need this to build both TensorFlow and
  PolicyGen.

  On my system, this is installed with `sudo pacman -S bazel`.

- [LibCURL](https://curl.haxx.se/libcurl/) - you need this to get item data from
  the WowDB API.

  On my system, this is installed with `sudo pacman -S curl`

- (OPTIONAL) [CUDA](https://developer.nvidia.com/cuda-downloads) and [cuDNN](https://developer.nvidia.com/cuDNN)
  Install these if you want CUDA support. Which you do. Duh.

  On my system, this is installed with `sudo pacman -S nvidia cuda cudnn`.

- (OPTIONAL) [Intel MKL](https://software.intel.com/en-us/mkl) - this is a
  useful tool for accelerating linear algebra if you're not planning on using a
  GPU. It might even be helpful for other things. Who knows?! You want this.

  On my system, this is installed with `yaourt -S intel-mkl`.

- [Tensorflow](https://github.com/tensorflow/tensorflow) - If you want maximum
  performance, compile this from source and run their TF configuration wizard.
  Here's how to achieve that:

  1) Install pip, numpy, and wheel.
     If you're on a distro that doesn't ship headers, you also need python-dev.
     If python3 isn't your default, you want the python3 versions of these.

     On my system, `sudo pacman -S python-pip python-numpy python-wheel`

  2) Clone the TF repo: `git clone https://github.com/tensorflor/tensorflow`

  3) Run the configuration script: `cd tensorflow && ./configure`

     You want to point the script to your install of CUDA.

     Tell the script to target the latest version of CUDA.
     (I had to change mine from a default of 9.0 to my installed version of 9.1).

     Tell the script to compile for the compute capability of your hardware
     device (you can see this by running the deviceQuery example in the CUDA
     extras/demo_suite directory).

     Tell the script to compile Tensorflow for your cuDNN version.

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

     `sudo pip install /tmp/tensorflow_pkg/tensorflow-{VERSION}.whl`

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
  directly - I haven't tried, but it seems likely to work.


### Developers

Policygen welcomes new contributors. There are opportunities to be found in any
codebase for improvement! In order to trivially resolve style differences, this
project provides a .clang-format and a .clang-tidy file that impose LINT checks
which MUST be followed before code is checked in. If you'd like to contribute to
the development of Policygen, please ensure that you are able to run
clang-format and clang-tidy. We provide static analysis tooling at
`tools/static_analysis.sh` which is run during continuous integration testing,
please run it yourself and ensure your code builds cleanly before submitting a
pull request.

Optional Dependencies for developers:
- [ClangFormat](https://clang.llvm.org/docs/ClangFormat.html) - all code
  is autoformatted to Google's style guide conventions.
- [ClangTidy](http://clang.llvm.org/extra/clang-tidy/) - all code is statically
  checked to ensure that minimal bugs make it through development.
- [Buildifier](https://github.com/bazelbuild/buildtools) - BUILD file is
  autoformatted to our style guide convensions.

To build a compilation database (necessary for clang-format and clang-tidy),
from the root of the repository run

`./tools/generate_compilationdb.sh`

To run static analysis, from the root of the repository run

`./tools/static_analysis.sh`

To run all unit tests, from the root of the repository run

`bazel test //...`

## Building Policygen

Policygen is built with `bazel`, which is Google's opensource version of our
internal build tool. Build artifacts are compiled into `./bazel-genfiles/` and
binaries are placed into `./bazel-bin/`.

To compile the simulation service, run

`bazel build --config=opt simulator:service`

To compile the learning module, run

`bazel build --config=opt learn:policygen`

## Running Policygen

To run the simulation service, invoke

`./bazel-bin/simulator/service`

This starts up a local RPC service on port 50051 by default (configurable).

To run policy generation, invoke

`./bazel-bin/learn/policygen $ENCOUNTER_CONFIG_PATH $CHARACTER_CONFIG_PATH`

where `$ENCOUNTER_CONFIG_PATH` contains details on the raid encounter and
`$CHARACTER_CONFIG_PATH` contains details on the player character (gear, etc).

## Policygen Architecture Overview

### Standalone Simulation Engine Binary

We implement a binary that takes a text-formatted simulation configuration as
an input file, and returns the result of running a single batch simulation.
This is mostly useful as a debugging tool.

To build and invoke the standalone engine binary, run this:

`bazel build simulator:standalone`

`./bazel-bin/simulator:standalone $FULL_CONFIG_PATH`

### Simulation Service

We implement an RPC service that receives SimulationRequests and produces
SimulationResponses. This is meant to be horizontally scalable. Each machine
running this service is running a multithreaded simulation server binary.
This is an embarassingly parallel problem, with minimal data dependencies.
Simulation is mostly a CPU-bound problem, although it might be an interesting
problem to attempt a "discrete event simulation" on GPU.

### Deep Reinforcement Learning Client

The learning client `policygen` takes in a configuration describing the
encounter to be simulated, as well as a configuration for the player character.

This component feeds simulation configurations to the simulation service to
evaluate the quality of an action policy. We learn an optimal policy for the
given encounter using deep Q learning. Finally, we return the (gearset, policy)
pair that performed the best.

### Game Client Data Parser (TODO)

We need to get client data out of the game files. They're encoded in weird
database formats, and we need this to be usable for our simulator. This is a
non-interesting, solved problem by the SimC team, but we need this component,
unless we want to hardcode every constant for every spell and every item. We
rely on this data being available at compile time in the simulation engine code.

### LUA Addon (TODO)

This component takes the trained action policy for the given gearset and
encounter configuration, and embeds itself directly into the player's in-game
interface: it uses the existing WoW API to build up an equivalent representation
of "player state" used by the action policy network, and then runs the model
forward to determine the optimal action at any moment in time. In practice,
we'll want to determine the optimal action at both *this instant* as well as a
few times in the future, to give the player a bit of predictive breathing room.
This is similar (almost exactly identical) to existing addons Ovale or
HeroRotation.
