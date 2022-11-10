_This is a template for [cargo-generate](https://cargo-generate.github.io/cargo-generate/)._
_Use with `cargo generate rksm/axum-yew-template`._

# rust-fullstack-playground

This is a full stack Rust web app using [axum](https://github.com/tokio-rs/axum) and [yew](https://yew.rs/).

## Usage

Run `./scripts/install-dependencies.sh` when you have first generated this project.

Run the dev version (auto-reloads server & client on file change) with `./scripts/dev.sh`.

Run the pre-compiled version with `./scripts/prod.sh`.

The app will start at https://localhost:8080 by default.


```shell
cargo install cargo-watch
cargo install trunk

rustup target add wasm32-unknown-unknown
```


(Чтобы видеть код под `#[cfg(target_arch = "wasm32")]`)
"~/.config/Code - Insiders/User/settings.json"
```
"rust-analyzer.cargo.target": "wasm32-unknown-unknown",
"rust-analyzer.checkOnSave.command": "clippy",
"rust-analyzer.cargoRunner": null,
"emmet.includeLanguages": {
    "rust": "html"
}
```