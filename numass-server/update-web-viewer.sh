#!/bin/bash
cd ../numass-viewers && \
    CARGO_TARGET_DIR=../numass-server/target-trunk trunk build --release --dist ../numass-server/dist && \
    cd ../numass-server

if [ $? -ne 0 ]; then
    echo "Error: frontend build failed!"
    exit 1
fi

cargo build --release --bin data-viewer-web
if [ $? -ne 0 ]; then
    echo "Error: backend build failed!"
    exit 1
fi


#stop data-viewer-web service if it exists
if systemctl list-unit-files | grep data-viewer-web; then
  # Stop the service
  sudo systemctl stop data-viewer-web
else
    sudo cp resources/services/data-viewer-web.service /etc/systemd/system/
fi
sudo cp target/release/data-viewer-web /usr/local/bin/
sudo systemctl start data-viewer-web