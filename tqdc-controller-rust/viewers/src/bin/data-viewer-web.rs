use std::{path::PathBuf, str::FromStr};

use actix_files::Files;
use actix_web::{get, web, App, HttpServer, Responder, post};
use data_viewer_web::backend::{expand_dir, ProcessRequest, process_file};

#[get("/api/files")]
async fn files() -> impl Responder {
    let files = expand_dir(PathBuf::from_str("/data/numass-server").unwrap());
    web::Json(files)
}

#[post("/api/process")]
async fn index(request: web::Json<ProcessRequest>) -> impl Responder {
    let actix_web::web::Json(ProcessRequest { filepath, params }) = request;
    web::Json(process_file(filepath, params).await)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    HttpServer::new(|| {
        App::new()
        .service(index)
        .service(files)
        .service(Files::new("/", "../dist").index_file("index.html"))
    })
    .bind(("0.0.0.0", 8085))?
    .run()
    .await
}