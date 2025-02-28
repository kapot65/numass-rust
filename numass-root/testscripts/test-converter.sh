#!/bin/bash
cargo build --release
g++ -o convert-to-root ../convert-to-root.cxx \
  `root-config --cflags --glibs` \
  -I../build/_deps/argparse-src/include \
  -I../bindings \
  -L../target/release \
  -l:libnumass_root.a \
  -O3 \
  -march=native \
  -fPIC

LD_LIBRARY_PATH=./target/release ./convert-to-root \
  "/data-fast/numass-server/2024_11/Tritium_3/set_1/p0(30s)(HV1=14000)" -o point.root