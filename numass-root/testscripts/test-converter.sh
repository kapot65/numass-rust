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
  "/data-fast/numass-server/2024_11/Tritium_3/set_1/p0(30s)(HV1=14000)" \
  -p "{\"algorithm\":{\"Trapezoid\":{\"left\":6,\"center\":15,\"right\":6,\"treshold\":16,\"min_length\":10,\"skip\":\"None\",\"reset_detection\":{\"window\":10,\"treshold\":800,\"size\":110}}},\"convert_to_kev\":true}" \
  --postprocess '{"cut_bad_blocks":true,"merge_splits_first":false,"merge_close_events":true,"ignore_borders":false,"ignore_channels":[false,false,false,false,false,false,false]}'