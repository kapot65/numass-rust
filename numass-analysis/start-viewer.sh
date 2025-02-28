#!/bin/bash
cd ../numass-viewers
export LD_LIBRARY_PATH=$HOME/projects/numass-root/target/release:$LD_LIBRARY_PATH
# cargo build
cargo run --release --bin data-viewer -- --directory /data-fast/numass-server/