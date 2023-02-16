#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] 

use data_viewer_web::{backend::point_to_chunks, point_viewer::PointViewer};
use clap::Parser;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Opt {
    filepath: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() {

    let args = Opt::parse();

    let filepath = args.filepath.unwrap_or_else(|| {
        rfd::FileDialog::new().pick_file().expect("no file choosen")
    });

    let chunks = point_to_chunks(&filepath).await;

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(std::fs::canonicalize(&filepath).unwrap().to_str().unwrap(), native_options, Box::new(|_| Box::new(PointViewer {
        chunks,
        current_chunk: 0,
    }))).unwrap();
}