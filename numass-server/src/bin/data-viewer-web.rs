use std::{
    env::temp_dir,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    time::Duration,
};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, RawQuery, State},
    response::Response,
    routing::{get, post},
};

#[cfg(not(debug_assertions))]
use axum::{body::Full, response::Html};

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use dataforge::{DFMessage, read_df_header_and_meta, read_df_message};
use numass_processing::{
    numass::{self, protos::rsb_event},
    postprocess::{PostProcessParams, post_process},
    preprocess::{Preprocess, PreprocessParams},
    process::{ProcessParams, extract_events},
    storage::FSRepr,
    types::NumassEvents,
    viewer::ToROOTOptions,
};
use numass_server::Opt;
use protobuf::Message;
use tower_http::services::ServeDir;

/// Rewrite all paths in the listing to be relative to the storage root, so
/// that absolute server paths never leak into API responses.
/// Paths outside of the root are left as is.
fn make_relative(listing: &mut FSRepr, root: &std::path::Path) {
    let path = match listing {
        FSRepr::File { path, .. } => path,
        FSRepr::Directory { path, .. } => path,
    };
    if let Ok(relative) = path.strip_prefix(root) {
        *path = relative.to_path_buf();
    }
    if let FSRepr::Directory { children, .. } = listing {
        for child in children {
            make_relative(child, root);
        }
    }
}

/// List `filepath` (relative to the storage root), keeping all paths in the
/// response relative to the root as well.
async fn ls_listing(args: Opt, filepath: PathBuf) -> Json<FSRepr> {
    let mut listing = FSRepr::ls(args.directory.join(filepath)).await;
    make_relative(&mut listing, &args.directory);
    Json(listing)
}

#[tokio::main]
async fn main() {
    let args = <numass_server::Opt as clap::Parser>::parse();
    let address = args.address;

    println!("Starting server at {}", address);

    // Storage root listing; shared by /api/root and /api/ls with an empty path.
    let root_listing =
        |State(args): State<Opt>| async move { ls_listing(args, PathBuf::new()).await };

    // build our application with a single route
    let app = Router::new()
        .route("/api/meta/*path", get(|State(args): State<Opt>, Path(filepath): Path<PathBuf>| async move {
            let mut point_file = tokio::fs::File::open(args.directory.join(filepath)).await.unwrap();
            axum::response::Json(read_df_header_and_meta::<serde_json::Value>(&mut point_file).await.ok().map(|(_, meta)| meta))
        }))
        .route("/api/ls", get(root_listing))
        .route("/api/ls/*path", get(|State(args): State<Opt>, Path(filepath): Path<PathBuf>| async move {
            ls_listing(args, filepath).await
        }))
        .route("/api/root", get(root_listing))
        .route("/api/modified/*path", get(|State(args): State<Opt>, Path(filepath): Path<PathBuf>| async move {
            let metadata = tokio::fs::metadata(args.directory.join(filepath)).await.unwrap();
            axum::response::Json(metadata.modified().unwrap())
        }))
        .route("/api/to-root", get(|
            State(args): State<Opt>,
            RawQuery(to_root_options): RawQuery, // TODO: change to valid for TFile::Open(url)
            | async move {

                // axum default queryparser doesn't have corresponding serializer
                let root_options = serde_qs::from_str::<ToROOTOptions>(&to_root_options.unwrap()).unwrap();

                let output = {
                    let mut hasher = DefaultHasher::new();
                    root_options.hash(&mut hasher);
                    let hash = hasher.finish();
                    temp_dir().join(format!("{}.root", hash))
                };

                let mut command = tokio::process::Command::new("convert-to-root");
                command
                    .arg(args.directory.join(&root_options.filepath))
                    .arg("--process")
                    .arg(serde_json::to_string(&root_options.process).unwrap())
                    .arg("--postprocess")
                    .arg(serde_json::to_string(&root_options.postprocess).unwrap())
                    .arg("--output")
                    .arg(&output);


                command.spawn().unwrap().wait().await.unwrap();

                let content = tokio::fs::read(&output).await.unwrap();
                // remove temp file
                tokio::fs::remove_file(output).await.unwrap();

                let out_name = numass_processing::utils::construct_filename(root_options.filepath.to_str().unwrap(), Some("root"));

                Response::builder()
                    .header("Content-Disposition", format!("attachment; filename=\"{out_name}\""))
                    .header("content-type", "application/octet-stream")
                    .body(Body::from(content))
                    .unwrap()
        }))
        .route("/api/process/*path", post(|
                State(args): State<Opt>,
                Path(filepath): Path<PathBuf>,
                Json((preprocess, process, postprocessing)): Json<(
                    PreprocessParams,
                    ProcessParams,
                    Option<PostProcessParams>,
                )>,
            | async move {

            let mut point_file = tokio::fs::File::open(args.directory.join(filepath)).await.unwrap();
            let amplitudes = if let Ok(DFMessage {
                meta,
                data,
            }) = read_df_message::<numass::NumassMeta>(&mut point_file).await {
                if let numass::NumassMeta::Reply(numass::Reply::AcquirePoint { .. }) = &meta {
                    let point = rsb_event::Point::parse_from_bytes(&data.unwrap()).unwrap(); // TODO: return None for bad parsing

                    let out = if let Some(postprocessing) = postprocessing {
                        Some(post_process(
                            extract_events(Some(meta), point, &preprocess, &process),
                            &postprocessing,
                        ))
                    } else {
                        Some(extract_events(Some(meta), point, &preprocess, &process))
                    };
                    rmp_serde::to_vec(&out).unwrap()
                } else {
                    rmp_serde::to_vec::<Option<(NumassEvents, Preprocess)>>(&None).unwrap() // TODO: send error instead of None
                }
            } else {
                rmp_serde::to_vec::<Option<(NumassEvents, Preprocess)>>(&None).unwrap() // TODO: send error instead of None
            };


            Response::builder()
                .header("content-type", "application/messagepack")
                .body(Body::from(amplitudes))
                .unwrap()
        }))
        .nest_service("/files", ServeDir::new(args.directory.clone()))
        .with_state(args);

    #[cfg(not(debug_assertions))]
    let app = app
        .route(
            "/",
            get(|| async { Html(include_str!("../../dist/index.html")) }),
        )
        .route(
            "/data-viewer.js",
            get(|| async {
                Response::builder()
                    .header("content-type", "application/javascript")
                    .body(Full::from(include_str!("../../dist/data-viewer.js")))
                    .unwrap()
            }),
        )
        .route(
            "/data-viewer_bg.wasm",
            get(|| async {
                Response::builder()
                    .header("content-type", "application/wasm")
                    .body(Body::from(
                        include_bytes!("../../dist/data-viewer_bg.wasm").to_vec(),
                    ))
                    .unwrap()
            }),
        )
        .route(
            "/worker.js",
            get(|| async {
                Response::builder()
                    .header("content-type", "application/javascript")
                    .body(Full::from(include_str!("../../dist/worker.js")))
                    .unwrap()
            }),
        )
        .route(
            "/worker_bg.wasm",
            get(|| async {
                Response::builder()
                    .header("content-type", "application/wasm")
                    .body(Body::from(
                        include_bytes!("../../dist/worker_bg.wasm").to_vec(),
                    ))
                    .unwrap()
            }),
        );

    axum::Server::bind(&address)
        .tcp_keepalive(Some(Duration::from_secs(600)))
        .serve(app.into_make_service())
        .await
        .unwrap();
}
