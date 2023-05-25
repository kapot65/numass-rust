use std::{sync::Arc};

use analysis::get_points_by_pattern;
use plotly::{common::Title, layout::Axis, Layout, Plot};
use processing::{
    numass::{protos::rsb_event, NumassMeta},
    histogram::PointHistogram, extract_amplitudes, Algorithm
};
use protobuf::Message;

use dataforge::read_df_message;
use tokio::sync::Mutex;

pub const DB_ROOT: &str = "/home/chernov/data";

#[tokio::main]
async fn main() {
    
    let db_root = "/home/chernov/data";
    let run = "2023_03";
    let pattern = format!("/{run}/Tritium_5/set_*/p*(30s)(HV1=14000)");
    let exclude = [];

    let points = get_points_by_pattern(db_root, &pattern, &exclude).first_key_value().unwrap().1.clone();

    let histogram_all = Arc::new(Mutex::new(PointHistogram::new_step(0.0..27.0, 0.1)));
    let histogram = Arc::new(Mutex::new(PointHistogram::new_step(0.0..27.0, 0.1)));

    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));
    let handles = points.iter().map(|filepath| {
        let filepath = filepath.clone();
        let histogram_all = Arc::clone(&histogram_all);
        let histogram = Arc::clone(&histogram);
        let pb = Arc::clone(&pb);

        tokio::spawn(async move {
            let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
            let message = read_df_message::<NumassMeta>(&mut point_file)
                .await
                .unwrap();

            let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

            let algorithm = Algorithm::default();

            let events = extract_amplitudes(
                &point, &algorithm, true
            );
            
            let amps = events.iter().filter_map(|(_, frames)| {
                if frames.len() == 1 
                || processing::check_neigbors_fast::<f32>(&frames) 
                // || frames.contains_key(&5)
                {
                    None
                } else {
                    Some(frames.into_iter().map(|(_, amp)| *amp))
                }
            }).flatten().collect::<Vec<_>>();

            histogram.lock().await.add_batch(0, amps);

            let amps_all = events.iter().flat_map(|(_, frames)| {
                frames.into_iter().map(|(_, amp)| *amp)
            }).collect::<Vec<_>>();

            histogram_all.lock().await.add_batch(0, amps_all);

            pb.lock().await.inc(1);
        })
    }).collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(
            format!("{pattern}")
                .as_str(),
        ))
        .x_axis(Axis::new().title(Title::new("time delta, ns")))
        // .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
        .height(1000);

    plot.set_layout(layout);
    
    histogram_all.try_lock().unwrap().draw_plotly(&mut plot, None);
    histogram.try_lock().unwrap().draw_plotly(&mut plot, None);
    
    plot.show();
}
