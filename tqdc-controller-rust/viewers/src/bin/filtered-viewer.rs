#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(target_arch = "wasm32")]
fn main() {todo!()}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    use clap::Parser;
    use viewers::{backend::filter_events, filtered_viewer::FilteredViewer};

    #[derive(Parser, Debug)]
    #[clap(author, version, about, long_about = None)]
    struct Opt {
        filepath: std::path::PathBuf,
        #[clap(long, default_value_t = 0.0)]
        min: f32,
        #[clap(long, default_value_t = 5.0)]
        max: f32,
        #[clap(long, default_value_t = 5000)]
        neigborhood: u64,
    }

    let args = Opt::parse();
    let filepath = args.filepath;
    let neigborhood = args.neigborhood;
    let range = args.min..args.max;
    let independent = filter_events(&filepath, &range, args.neigborhood).await;

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        format!("filtered {filepath:?} ({range:?} keV, {neigborhood} ns neigborhood)").as_str(),
        native_options,
        Box::new(|_| {
            Box::new(FilteredViewer {
                independent,
                current: 0,
            })
        }),
    )
    .unwrap();
}
