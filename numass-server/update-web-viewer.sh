#!/bin/bash
cd ../numass-viewers && \
    CARGO_TARGET_DIR=../numass-server/target-trunk trunk build --release --dist ../numass-server/dist && \
    cd ../numass-server
cargo build --release --bin data-viewer-web
sudo systemctl stop data-viewer-web
sudo cp target/release/data-viewer-web /usr/local/bin/
sudo systemctl start data-viewer-web