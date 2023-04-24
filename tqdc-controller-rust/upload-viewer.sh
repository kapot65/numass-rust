#!/bin/bash
cd viewers && trunk build --release --dist ../dist && cd ..
cross build --target x86_64-unknown-linux-gnu --release --bin data-viewer-web
scp target/x86_64-unknown-linux-gnu/release/data-viewer-web 192.168.111.1:~/