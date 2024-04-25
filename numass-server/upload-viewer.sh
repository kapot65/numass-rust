#!/bin/bash
cd ../numass-viewers && \
    CARGO_TARGET_DIR=../numass-server/target-trunk trunk build --release --dist ../numass-server/dist && \
    cd ../numass-server
CARGO_TARGET_DIR=target-cross cross build --target x86_64-unknown-linux-gnu --release --bin data-viewer-web
scp target-cross/x86_64-unknown-linux-gnu/release/data-viewer-web 192.168.111.1:~/