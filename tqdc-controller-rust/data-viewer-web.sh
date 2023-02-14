#!/bin/bash
cd viewers && trunk build --release --dist ../dist
cargo run --release --bin data-viewer-web