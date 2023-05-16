#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("this binary is not meant to be run in browser")
}

#[cfg(not(target_arch = "wasm32"))]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use std::{net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};

    #[cfg(not(debug_assertions))]
    use actix_web::{get, web::Bytes};
    use actix_web::{
        http::header::ContentType,
        post,
        web::{self, Data},
        App, HttpResponse, HttpServer, Responder,
    };
    use clap::Parser;

    use backend::{
        expand_dir, process_file, ProcessRequest,
        CACHE_DIRECTORY,
    };

    #[derive(Parser, Debug, Clone)]
    #[clap(author, version, about, long_about = None)]
    struct Opt {
        directory: PathBuf,
        #[clap(long, default_value_t = SocketAddr::from_str("0.0.0.0:8085").unwrap())]
        address: SocketAddr,
        #[clap(long)]
        cache_directory: Option<String>,
    }

    #[post("/api/process")]
    async fn process(request: web::Json<ProcessRequest>) -> impl Responder {
        let actix_web::web::Json(reqest) = request;

        match reqest {
            ProcessRequest::CalcHist { filepath, processing } => HttpResponse::Ok()
                .content_type(ContentType::json())
                .body(serde_json::to_string(&process_file(filepath, processing)).unwrap()),
            _ => HttpResponse::BadRequest().body(""),
        }
    }

    #[cfg(not(debug_assertions))]
    #[get("/")]
    async fn index() -> impl Responder {
        HttpResponse::Ok()
            .content_type(ContentType::html())
            .body(include_str!("../../dist/index.html"))
    }

    #[cfg(not(debug_assertions))]
    #[get("/data-viewer.js")]
    async fn js() -> impl Responder {
        HttpResponse::Ok()
            .content_type(ContentType(mime::APPLICATION_JAVASCRIPT))
            .body(include_str!("../../dist/data-viewer.js"))
    }

    #[cfg(not(debug_assertions))]
    #[get("/data-viewer_bg.wasm")]
    async fn wasm() -> impl Responder {
        HttpResponse::Ok()
            .content_type("application/wasm")
            .body(Bytes::from_static(include_bytes!(
                "../../dist/data-viewer_bg.wasm"
            )))
    }

    let args = Opt::parse();

    if let Some(cache_directory) = args.cache_directory {
        if std::env::var(CACHE_DIRECTORY).is_err() {
            std::env::set_var(CACHE_DIRECTORY, cache_directory)
        } else {
            panic!("cache directory is set via CLI and ENV at the same time!")
        }
    }

    HttpServer::new(move || {
        let app = App::new()
            .app_data(Data::new(args.directory.clone()))
            .route(
                "/api/files",
                web::get().to(|directory: web::Data<PathBuf>| async move {
                    let files = expand_dir(PathBuf::clone(&directory));
                    web::Json(files)
                }),
            )
            .service(process)
            .service(
                actix_files::Files::new(
                    &format!("/files{}", args.directory.to_str().unwrap()),
                    &args.directory,
                )
                .show_files_listing(),
            );
        #[cfg(not(debug_assertions))]
        {
            app.service(index).service(js).service(wasm)
        }
        #[cfg(debug_assertions)]
        {
            app
        }
    })
    .keep_alive(Duration::from_secs(600))
    .bind(args.address)?
    .run()
    .await
}
