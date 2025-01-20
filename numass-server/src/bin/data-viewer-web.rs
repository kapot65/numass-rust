use std::{path::PathBuf, time::Duration, collections::hash_map::DefaultHasher, hash::{Hash, Hasher}};

use axum::{
    routing::{get, post},
    response::Response,
    body::Body,
    Router, Json, extract::{State, Path},
};

#[cfg(not(debug_assertions))]
use axum::{response::Html, body::Full};

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use backend::Opt;
use dataforge::{DFMessage, read_df_message, read_df_header_and_meta};
use processing::{numass::{self, protos::rsb_event}, preprocess::Preprocess, process::{extract_events, Algorithm, ProcessParams}, storage::FSRepr, types::NumassEvents};
use protobuf::Message;
use tower_http::services::ServeDir;

fn is_default_params(params: &ProcessParams) -> bool {
    params.algorithm == Algorithm::default() && params.convert_to_kev
}

#[tokio::main]
async fn main() {

    let args = <backend::Opt as clap::Parser>::parse();
    let address = args.address;

    println!("Starting server at {}", address);

    // build our application with a single route
    let app = Router::new()
        .route("/api/meta/*path", get(|Path(filepath): Path<PathBuf>| async move {
            let mut point_file = tokio::fs::File::open(PathBuf::from("/").join(filepath)).await.unwrap();
            axum::response::Json(read_df_header_and_meta::<serde_json::Value>(&mut point_file).await.ok().map(|(_, meta)| meta))
        }))
        .route("/api/ls/*path", get(|Path(filepath): Path<PathBuf>| async move {
            let filepath = PathBuf::from("/").join(filepath);
            axum::response::Json(FSRepr::ls(filepath).await)
        }))
        .route("/api/root", get(|State(args): State<Opt>| async move {
            Json(FSRepr::ls(args.directory).await)
        }))
        .route("/api/modified/*path", get(|Path(filepath): Path<PathBuf>| async move {
            let metadata = tokio::fs::metadata(PathBuf::from("/").join(filepath)).await.unwrap();
            axum::response::Json(metadata.modified().unwrap())
        }))
        .route("/api/process/*path", post(|
                State(args): State<Opt>,
                Path(filepath): Path<PathBuf>, 
                Json(processing): Json<ProcessParams>,
            | async move {

            let hash = {
                let mut hasher = DefaultHasher::new();
                filepath.hash(&mut hasher);
                processing.hash(&mut hasher);
                hasher.finish()
            };

            let read_amplitudes = {
                let cache_directory = args.cache_directory.clone();
                let key = hash.to_string();
                || async move {
                    // TODO: switch to functions from processing::storage
                    let mut point_file = tokio::fs::File::open(PathBuf::from("/").join(filepath)).await.unwrap(); 
                    if let Ok(DFMessage {
                        meta,
                        data,
                    }) = read_df_message::<numass::NumassMeta>(&mut point_file).await {
                        if let numass::NumassMeta::Reply(numass::Reply::AcquirePoint { .. }) = &meta {
                            let point = rsb_event::Point::parse_from_bytes(&data.unwrap()).unwrap(); // return None for bad parsing
                            let out = Some(extract_events(
                                Some(meta),
                                point,
                                &processing,
                            ));
                            let processed = rmp_serde::to_vec(&out).unwrap();

                            if is_default_params(&processing) {
                                if let Some(cache_directory) = cache_directory {
                                    cacache::write(cache_directory, key, &processed).await.unwrap();
                                }
                            }
                            processed
                        } else {
                            rmp_serde::to_vec::<Option<(NumassEvents, Preprocess)>>(&None).unwrap() // TODO: send error instead of None
                        }
                    } else {
                        rmp_serde::to_vec::<Option<(NumassEvents, Preprocess)>>(&None).unwrap() // TODO: send error instead of None
                    }
                }
            };

            let amplitudes = if let Some(cache_directory) = args.cache_directory {
                if let Ok(data) = cacache::read(cache_directory, &hash.to_string()).await {
                    data
                } else {
                    read_amplitudes().await
                }
            } else {
                read_amplitudes().await
            };
            
            Response::builder()
                .header("content-type", "application/messagepack")
                .body(Body::from(amplitudes))
                .unwrap()
        }))
        .nest_service(&format!("/files{}", args.directory.to_str().unwrap()), ServeDir::new(args.directory.clone()))
        .with_state(args);

    #[cfg(not(debug_assertions))]
    let app = app.route("/", get(|| async  {
        Html(include_str!("../../dist/index.html"))
    })).route("/data-viewer.js", get(|| async  {
        Response::builder()
            .header("content-type", "application/javascript")
            .body(Full::from(include_str!("../../dist/data-viewer.js")))
            .unwrap()
    })).route("/data-viewer_bg.wasm", get(|| async  {
        Response::builder()
            .header("content-type", "application/wasm")
            .body(Body::from(include_bytes!("../../dist/data-viewer_bg.wasm").to_vec()))
            .unwrap()
    })).route("/worker.js", get(|| async  {
        Response::builder()
            .header("content-type", "application/javascript")
            .body(Full::from(include_str!("../../dist/worker.js")))
            .unwrap()
    })).route("/worker_bg.wasm", get(|| async  {
        Response::builder()
            .header("content-type", "application/wasm")
            .body(Body::from(include_bytes!("../../dist/worker_bg.wasm").to_vec()))
            .unwrap()
    }));


    axum::Server::bind(&address)
        .tcp_keepalive(Some(Duration::from_secs(600)))
        .serve(app.into_make_service())
        .await
        .unwrap();
}
