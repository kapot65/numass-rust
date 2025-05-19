use std::{env::temp_dir, hash::{DefaultHasher, Hash, Hasher}, path::PathBuf, time::Duration};

use axum::{
    body::Body, extract::{Path, RawQuery, State}, response::Response, routing::{get, post}, Json, Router
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
use processing::{numass::{self, protos::rsb_event}, postprocess::{post_process, PostProcessParams}, preprocess::Preprocess, process::{extract_events, ProcessParams}, storage::FSRepr, types::NumassEvents, viewer::ToROOTOptions};
use protobuf::Message;
use tower_http::services::ServeDir;

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
        .route("/api/to-root", get(|
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
                    .arg(&root_options.filepath)
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

                let out_name = processing::utils::construct_filename(root_options.filepath.to_str().unwrap(), Some("root"));

                Response::builder()
                    .header("Content-Disposition", format!("attachment; filename=\"{out_name}\""))
                    .header("content-type", "application/octet-stream")
                    .body(Body::from(content))
                    .unwrap()
        }))
        .route("/api/process/*path", post(|
                Path(filepath): Path<PathBuf>, 
                Json((process, postprocessing)): Json<(ProcessParams, Option<PostProcessParams>)>,
            | async move {

            let mut point_file = tokio::fs::File::open(PathBuf::from("/").join(filepath)).await.unwrap(); 
            let amplitudes = if let Ok(DFMessage {
                meta,
                data,
            }) = read_df_message::<numass::NumassMeta>(&mut point_file).await {
                if let numass::NumassMeta::Reply(numass::Reply::AcquirePoint { .. }) = &meta {
                    let point = rsb_event::Point::parse_from_bytes(&data.unwrap()).unwrap(); // TODO: return None for bad parsing

                    let out = if let Some(postprocessing) = postprocessing {
                        Some(post_process(extract_events(Some(meta), point, &process), &postprocessing))
                    } else {
                        Some(extract_events(
                            Some(meta),
                            point,
                            &process,
                        ))
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
