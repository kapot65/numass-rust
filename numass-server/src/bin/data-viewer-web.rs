use std::{path::PathBuf, time::Duration};

use axum::{
    routing::{get, post},
    response::Response,
    body::Body,
    Router, Json, extract::{State, Path},
};

#[cfg(not(debug_assertions))]
use axum::{response::Html, body::Full};

use backend::Opt;
use dataforge::{DFMessage, read_df_message, read_df_header_and_meta};
use processing::{Algorithm, ProcessParams, viewer::FSRepr, extract_events, numass::{self, protos::rsb_event}, NumassAmps};
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
        .route("/api/modified/*path", get(|Path(filepath): Path<PathBuf>| async move {
            let metadata = tokio::fs::metadata(PathBuf::from("/").join(filepath)).await.unwrap();
            axum::response::Json(metadata.modified().unwrap())
        }))
        .route("/api/process/*path", post(|
                State(args): State<Opt>,
                Path(filepath): Path<PathBuf>, 
                Json(processing): Json<ProcessParams>,
            | async move {

            let key = filepath.clone().into_os_string().into_string().unwrap();

            let read_amplitudes = {
                let cache_directory = args.cache_directory.clone();
                let key = key.clone();
                || async move {
                    let mut point_file = tokio::fs::File::open(PathBuf::from("/").join(filepath)).await.unwrap(); 
                    if let Ok(DFMessage {
                        meta: numass::NumassMeta::Reply(numass::Reply::AcquirePoint { .. }),
                        data,
                    }) = read_df_message::<numass::NumassMeta>(&mut point_file).await {
                        let point = rsb_event::Point::parse_from_bytes(&data.unwrap()).unwrap(); // return None for bad parsing
                        let out = Some(extract_events(
                            &point,
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
                        rmp_serde::to_vec::<Option<NumassAmps>>(&None).unwrap()
                    }
                }
            };

            let amplitudes = if let Some(cache_directory) = args.cache_directory {
                if let Ok(data) = cacache::read(cache_directory, &key).await {
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
        .route("/api/files", get(|State(args): State<Opt>| async move {
            Json(FSRepr::expand_dir(args.directory))
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
