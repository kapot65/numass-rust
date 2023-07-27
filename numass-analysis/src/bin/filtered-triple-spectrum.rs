use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

use analysis::get_points_by_pattern;
use plotly::{common::Title, layout::Axis, Layout, Plot};
use processing::{
    numass::{protos::rsb_event, NumassMeta},
    histogram::PointHistogram, extract_amplitudes, ProcessParams
};
use protobuf::Message;

use dataforge::read_df_message;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    
    let db_root = "/data-ssd";
    let run = "2023_03";
    let pattern = format!("/{run}/Tritium_2/set_1/p118(30s)(HV1=12000)");
    // let pattern = format!("/{run}/Tritium_2/set_*/p*(30s)(HV1=12000)");
    let exclude = [];

    let points = get_points_by_pattern(db_root, &pattern, &exclude).first_key_value().unwrap().1.clone();

    let histogram_all = Arc::new(Mutex::new(PointHistogram::new_step(-2.0..40.0, 0.1)));
    let histogram = Arc::new(Mutex::new(PointHistogram::new_step(-2.0..40.0, 0.1)));

    let counts = Arc::new(AtomicUsize::new(0));
    let counts_all = Arc::new(AtomicUsize::new(0));

    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));
    let handles = points.iter().map(|filepath| {
        let filepath = filepath.clone();
        let histogram_all = Arc::clone(&histogram_all);
        let histogram = Arc::clone(&histogram);
        let pb = Arc::clone(&pb);

        let counts = Arc::clone(&counts);
        let counts_all = Arc::clone(&counts_all);

        tokio::spawn(async move {
            let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
            let message = read_df_message::<NumassMeta>(&mut point_file)
                .await
                .unwrap();

            let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

            let processing = ProcessParams::default();

            let events = extract_amplitudes(
                &point, &processing
            );
            
            let amps = events.iter().filter_map(|(_, amps)| {
                if  amps.len() == 1 ||
                    processing::check_neigbors_fast::<f32>(amps) 
                { None } else {
                    Some(amps.values().sum::<f32>())
                }
            }).collect::<Vec<_>>();

            // let amps = events.iter().filter_map(|(_, amps)| {
            //     if  amps.len() == 1 || 
            //         // frames.len() > 2 ||
            //         processing::check_neigbors_fast::<f32>(&amps) 
            //     { None } else {
            //         Some(amps.into_iter().map(|(_, amp)| *amp).sum::<f32>())
            //     }
            // }).collect::<Vec<_>>();

            counts.store(counts.load(Ordering::Relaxed) + amps.len(), Ordering::Relaxed);
            histogram.lock().await.add_batch(0, amps);

            let amps_all = events.iter()
            .filter_map(|(_, frames)| {
                if frames.len() > 1 && frames.contains_key(&5) {
                    Some(frames.values().sum::<f32>())
                } else { None }
            }).collect::<Vec<_>>();

            counts_all.store(counts_all.load(Ordering::Relaxed) + amps_all.len(), Ordering::Relaxed);
            histogram_all.lock().await.add_batch(0, amps_all);

            pb.lock().await.inc(1);
        })
    }).collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    println!("Total events: {}", counts.load(Ordering::Relaxed));
    println!("Total events: {}", counts_all.load(Ordering::Relaxed));

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(&pattern))
        .x_axis(Axis::new().title(Title::new("Enery, KeV")))
        .height(1000);
    plot.set_layout(layout);

    histogram_all.try_lock().unwrap().draw_plotly(&mut plot, None);
    
    {
        let histogram = histogram.try_lock().unwrap();
        let under_hist = histogram.events_all(Some(10.2..19.0));
        let total = histogram.events_all(Some(4.0..40.0));
        println!("{}", under_hist as f32 / total as f32);
        histogram.draw_plotly(&mut plot, None);
    }
    
    plot.show();
}
