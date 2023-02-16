#!/bin/bash
cd viewers && trunk build --release --dist ../dist && cd ..
cargo build --release --bin data-viewer-web
sudo systemctl stop data-viewer-web.service
sudo cp target/release/data-viewer-web /usr/local/bin/
sudo systemctl start data-viewer-web.service