#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] use std::path::PathBuf;

// hide console window on Windows in release
use data_viewer_web::app;

use clap::Parser;
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    #[clap(long)]
    directory: Option<PathBuf>,
}

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> eframe::Result<()> {
    use data_viewer_web::backend::expand_dir;

    let opt = Opt::parse();

    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "data-viewer",
        native_options,
        Box::new(|_| {
            let app = app::DataViewerApp::new();
            if let Some(directory) = opt.directory {
                *app.root.lock() = Some(expand_dir(directory))
            }
            Box::new(app)
        }),
    )
}

// when compiling to web using trunk.
#[cfg(target_arch = "wasm32")]
fn main() {
    // Make sure panics are logged using `console.error`.
    console_error_panic_hook::set_once();

    // Redirect tracing to console.log and friends:
    tracing_wasm::set_as_global_default();

    let web_options = eframe::WebOptions::default();
    wasm_bindgen_futures::spawn_local(async {
        eframe::start_web(
            "the_canvas_id", // hardcode it
            web_options,
            Box::new(|_| {
                let app = app::DataViewerApp::new();
                Box::new(app)
            }),
        )
        .await
        .expect("failed to start eframe");
    });
}
