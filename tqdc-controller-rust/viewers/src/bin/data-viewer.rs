#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use viewers::app;

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> eframe::Result<()> {
    use viewers::{backend::expand_dir, CACHE_DIRECTORY};
    use {clap::Parser, std::path::PathBuf};

    #[derive(Parser, Debug)]
    #[clap(author, version, about, long_about = None)]
    struct Opt {
        #[clap(long)]
        directory: Option<PathBuf>,
        #[clap(long)]
        cache_directory: Option<String>,
    }

    // abort programm if any of threads panic
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        std::process::exit(1);
    }));

    let opt = Opt::parse();
    if let Some(cache_directory) = opt.cache_directory {
        if std::env::var(CACHE_DIRECTORY).is_err() {
            std::env::set_var(CACHE_DIRECTORY, cache_directory)
        } else {
            panic!("cache directory is set via CLI and ENV at the same time!")
        }
    }

    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::init();

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "data-viewer",
        native_options,
        Box::new(|_| {
            let app = app::DataViewerApp::new();
            if let Some(directory) = opt.directory {
                *app.root.lock() = expand_dir(directory)
            }
            Box::new(app)
        }),
    )
}

// when compiling to web using trunk.
#[cfg(target_arch = "wasm32")]
fn main() {
    // Make sure panics are logged using `console.error`.

    use std::io::Cursor;

    use dataforge::{read_df_message_sync, DFMessage};
    use eframe::web_sys::window;
    use gloo_net::http::Request;
    use viewers::{
        backend::{DeviceFrame, ProcessRequest},
        filtered_viewer, point_viewer,
    };
    use wasm_bindgen_futures::spawn_local;
    console_error_panic_hook::set_once();

    // Redirect tracing to console.log and friends:
    tracing_wasm::set_as_global_default();

    let request = match window().unwrap().location().search() {
        Ok(search) => {
            let search = search.trim_start_matches('?');
            serde_qs::from_str::<ProcessRequest>(search).ok()
        }
        _ => None,
    };

    let web_options = eframe::WebOptions::default();
    if let Some(ProcessRequest::FilterEvents {
        filepath,
        range,
        neigborhood,
    }) = request
    {
        window().unwrap().document().unwrap().set_title(
            format!("filtered {filepath:?} ({range:?} keV, {neigborhood} ns neigborhood)").as_str(),
        );

        spawn_local(async move {
            let independent = rmp_serde::from_slice::<Vec<(DeviceFrame, Vec<DeviceFrame>)>>(
                &Request::post("/api/process")
                    .json(&ProcessRequest::FilterEvents {
                        filepath,
                        range,
                        neigborhood,
                    })
                    .unwrap()
                    .send()
                    .await
                    .unwrap()
                    .binary()
                    .await
                    .unwrap(),
            )
            .unwrap();

            eframe::start_web(
                "the_canvas_id", // hardcode it
                web_options,
                Box::new(|_| {
                    let app = filtered_viewer::FilteredViewer {
                        current: 0,
                        independent,
                    };
                    Box::new(app)
                }),
            )
            .await
            .expect("failed to start eframe");
        })
    } else if let Some(ProcessRequest::SplitTimeChunks { filepath }) = request {
        window()
            .unwrap()
            .document()
            .unwrap()
            .set_title(filepath.to_str().unwrap());
        spawn_local(async move {
            use processing::numass::{protos::rsb_event, NumassMeta};
            use protobuf::Message;
            let point_data = Request::get(&format!("/files{}", filepath.to_str().unwrap()))
                .send()
                .await
                .unwrap()
                .binary()
                .await
                .unwrap();

            let mut buf = Cursor::new(point_data);
            let message: DFMessage<NumassMeta> =
                read_df_message_sync::<NumassMeta>(&mut buf).unwrap();
            let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
            let chunks = viewers::backend::point_to_chunks(point);

            eframe::start_web(
                "the_canvas_id", // hardcode it
                web_options,
                Box::new(|_| {
                    let app = point_viewer::PointViewer {
                        current_chunk: 0,
                        chunks,
                    };
                    Box::new(app)
                }),
            )
            .await
            .expect("failed to start eframe");
        })
    } else {
        spawn_local(async {
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
}
