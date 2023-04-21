```shell
cargo install trunk

rustup target add wasm32-unknown-unknown
```

Зависимости для gui
sudo apt install libgtk-3-dev # (rfd)
sudo apt install libfontconfig-dev
sudo apt install build-essential

## cross compilation
1. install prerequirements
    ```shell
    sudo apt install podman
    cargo install cross --locked
    ```
2. cross compile server
    ```shell
    cd viewers && trunk build --release --dist ../dist && cd ..
    cross build --target x86_64-unknown-linux-gnu --release --bin data-viewer-web
    scp target/x86_64-unknown-linux-gnu/release/data-viewer-web 192.168.111.1:~/

    #move executable from home folder to /usr/local/bin and restart data-viewer-web service
    ```


