#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(target_arch = "wasm32")]
fn main() {
    todo!()
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
    use clap::Parser;
    use protobuf::Message;

    use dataforge::read_df_message;
    use processing::numass::{protos::rsb_event, NumassMeta};

    use backend::point_to_chunks;
    use viewers::point_viewer::PointViewer;

    #[derive(Parser, Debug)]
    #[clap(author, version, about, long_about = None)]
    struct Opt {
        filepath: Option<std::path::PathBuf>,
    }

    let args = Opt::parse();

    let filepath = args
        .filepath
        .unwrap_or_else(|| rfd::FileDialog::new().pick_file().expect("no file choosen"));

    let point = {
        let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
        let message = read_df_message::<NumassMeta>(&mut point_file)
            .await
            .unwrap();
        rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap()
    };

    let chunks = point_to_chunks(point);

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        std::fs::canonicalize(&filepath).unwrap().to_str().unwrap(),
        native_options,
        Box::new(|_| {
            Box::new(PointViewer {
                chunks,
                current_chunk: 0,
            })
        }),
    )
    .unwrap();
}
