use std::{
    env::temp_dir,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    time::Duration,
};

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, RawQuery, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{any, get},
};

#[cfg(not(debug_assertions))]
use axum::response::Html;

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use dataforge::{DFMessage, read_df_header_and_meta, read_df_message};
use dav_server::{DavHandler, DavMethodSet, localfs::LocalFs};
use numass_processing::{
    numass::{self, protos::rsb_event},
    postprocess::post_process,
    preprocess::Preprocess,
    process::{HistParams, extract_events},
    types::NumassEvents,
    viewer::ToROOTOptions,
};
use numass_server::Opt;
use protobuf::Message;
use socket2::{SockRef, TcpKeepalive};

/// Files are read-only at the method set. The data directory on disk may
/// still be writable by the account that runs the server.
async fn handle_dav(Extension(dav): Extension<DavHandler>, request: Request) -> impl IntoResponse {
    dav.handle(request).await
}

#[tokio::main]
async fn main() {
    let args = <numass_server::Opt as clap::Parser>::parse();
    let address = args.address;

    println!("Starting server at {}", address);

    let dav = DavHandler::builder()
        .filesystem(LocalFs::new(&args.directory, false, false, false))
        .strip_prefix("/files")
        .methods(DavMethodSet::WEBDAV_RO)
        .hide_symlinks(true)
        .build_handler();

    // build our application with a single route
    let app = Router::new()
        .route("/api/to-root", get(|
            State(args): State<Opt>,
            RawQuery(to_root_options): RawQuery, // TODO: change to valid for TFile::Open(url)
            | async move {

                // Голый serde_qs теряет пустые варианты вроде Average и Invert.
                let Some(query) = to_root_options else {
                    return StatusCode::BAD_REQUEST.into_response();
                };
                let Ok(root_options) = ToROOTOptions::from_query_string(&query) else {
                    return StatusCode::BAD_REQUEST.into_response();
                };

                let output = {
                    let mut hasher = DefaultHasher::new();
                    root_options.hash(&mut hasher);
                    let hash = hasher.finish();
                    temp_dir().join(format!("{}.root", hash))
                };

                let mut command = tokio::process::Command::new("convert-to-root");
                command
                    .arg(args.directory.join(&root_options.filepath))
                    .arg("--preprocess")
                    .arg(serde_json::to_string(&root_options.preprocess).unwrap())
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
        .route("/api/process/meta/{*path}", get(|
                State(args): State<Opt>,
                Path(filepath): Path<PathBuf>,
            | async move {
            let Ok(mut point_file) = tokio::fs::File::open(args.directory.join(filepath)).await else {
                return StatusCode::NOT_FOUND.into_response();
            };
            let meta = read_df_header_and_meta::<serde_json::Value>(&mut point_file)
                .await
                .ok()
                .map(|(_, meta)| meta);
            Json(meta).into_response()
        }))
        .route("/api/process/hist/{*path}", get(|
                State(args): State<Opt>,
                Path(filepath): Path<PathBuf>,
                RawQuery(query): RawQuery,
            | async move {
            let Ok(HistParams {
                preprocess,
                process,
                postprocess: postprocessing,
            }) = HistParams::from_query_string(&query.unwrap_or_default())
            else {
                return StatusCode::BAD_REQUEST.into_response();
            };

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
        .route("/files", any(handle_dav))
        .route("/files/", any(handle_dav))
        .route("/files/{*path}", any(handle_dav))
        .layer(Extension(dav))
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
                    .body(Body::from(include_str!("../../dist/data-viewer.js")))
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
                    .body(Body::from(include_str!("../../dist/worker.js")))
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

    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    SockRef::from(&listener)
        .set_tcp_keepalive(&TcpKeepalive::new().with_time(Duration::from_secs(600)))
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
