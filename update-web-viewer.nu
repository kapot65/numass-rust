#!/usr/bin/env nu

# Build and deploy the data-viewer-web frontend + backend.
#
# Native (default): build on this machine and deploy to /usr/local/bin.
#   numass-server/build.rs runs trunk on numass-viewers and embeds the wasm UI.
# Remote: pass the target IP to cross-compile (x86_64-unknown-linux-gnu), copy
#   the binary via scp and restart the service over ssh.

def main [ip?: string] {
    let root = $env.FILE_PWD
    cd $root

    if ($ip == null) {
        deploy-native $root
    } else {
        deploy-remote $root $ip
    }
}

def deploy-native [root: string] {
    # build.rs runs trunk and embeds the wasm UI; output is streamed.
    cargo build --release --bin data-viewer-web
    check-last "data-viewer-web build"

    let unit = ($root | path join "numass-server" "resources" "services" "data-viewer-web.service")
    let unit_installed = (systemctl list-unit-files | str contains "data-viewer-web")
    if $unit_installed {
        try { sudo systemctl stop data-viewer-web }
    } else {
        sudo cp $unit /etc/systemd/system/
    }

    sudo cp ($root | path join "target" "release" "data-viewer-web") /usr/local/bin/
    sudo systemctl start data-viewer-web
}

def deploy-remote [root: string, ip: string] {
    # build.rs can't run trunk inside the cross container, so build the wasm
    # assets natively first (trunk needs Trunk.toml + index.html in its cwd).
    cd ($root | path join "numass-viewers")
    with-env { CARGO_TARGET_DIR: ($root | path join "target-trunk") } {
        trunk build --release --dist ($root | path join "numass-server" "dist")
    }
    check-last "trunk build"
    cd $root

    with-env { CARGO_TARGET_DIR: ($root | path join "target-cross") } {
        cross build --target x86_64-unknown-linux-gnu --release --bin data-viewer-web
    }
    check-last "cross build"

    let bin = ($root | path join "target-cross" "x86_64-unknown-linux-gnu" "release" "data-viewer-web")
    scp $bin $"($ip):~/"
    check-last "scp"

    ssh $ip 'sudo systemctl stop data-viewer-web && sudo cp ~/data-viewer-web /usr/local/bin/ && sudo systemctl start data-viewer-web'
    check-last "ssh"
}

def check-last [what: string] {
    if $env.LAST_EXIT_CODE != 0 {
        print -e $"Error: ($what) failed!"
        exit 1
    }
}
