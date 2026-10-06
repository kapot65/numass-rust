#!/usr/bin/env nu

# Build and deploy the data-viewer-web frontend + backend.
#
# 1. Build data-viewer-web. numass-server/build.rs runs trunk on numass-viewers
#    first and writes the wasm UI to numass-server/dist (embedded via
#    include_str!/include_bytes!). Trunk uses <workspace>/target-trunk so it
#    does not lock the workspace target directory.
# 2. Install the systemd unit if missing, otherwise stop the running instance.
# 3. Copy the binary to /usr/local/bin and (re)start the service.

def main [] {
    let root = $env.FILE_PWD
    cd $root

    # Frontend + backend. Output is streamed; build.rs prints the trunk log.
    cargo build --release --bin data-viewer-web
    if $env.LAST_EXIT_CODE != 0 {
        print "Error: data-viewer-web build failed!"
        exit 1
    }

    # 2. systemd unit — install if missing, otherwise stop the running instance.
    let unit_installed = (systemctl list-unit-files | str contains "data-viewer-web")
    if $unit_installed {
        try { sudo systemctl stop data-viewer-web }
    } else {
        sudo cp ($root | path join "numass-server" "resources" "services" "data-viewer-web.service") /etc/systemd/system/
    }

    # 3. Deploy the binary and start the service.
    sudo cp ($root | path join "target" "release" "data-viewer-web") /usr/local/bin/
    sudo systemctl start data-viewer-web
}
