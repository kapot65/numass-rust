#!/bin/bash
sudo systemctl stop data-viewer-web
sudo cp ~/data-viewer-web /usr/local/bin/
sudo systemctl start data-viewer-web