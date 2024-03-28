use std::{collections::BTreeMap, sync::Arc};

use tokio::sync::Mutex;
use plotly::{common::Title, layout::Axis, Layout, Plot};

use analysis::{get_points_by_pattern, workspace::get_db_fast_root};
use processing::{
    histogram::PointHistogram, 
    process::{Algorithm, convert_to_kev, process_waveform, waveform_to_events}, 
    storage::load_point,
    types::ProcessedWaveform
};

#[tokio::main]
async fn main() {
    
    let db_root = get_db_fast_root();
    let run = "2023_03";
    let pattern = format!("/{run}/Tritium_5/set_*/p*(30s)(HV1=12000)");
    let exclude = [];

    let points = get_points_by_pattern(db_root.to_str().unwrap(), &pattern, &exclude).first_key_value().unwrap().1.clone();

    let range = 0.0..6.5;
    // let range = 6.5..27.0;

    let histogram = Arc::new(
        Mutex::new(PointHistogram::new_step(0.0..40e3, 24.0 * 6.0)));

    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));
    let handles = points.iter().map(|filepath| {
        
        let filepath = filepath.clone();
        let histogram = Arc::clone(&histogram);
        let pb = Arc::clone(&pb);
        let range = range.clone();

        tokio::spawn(async move {
            let point = load_point(&filepath).await;

            let mut independent: BTreeMap<u64, BTreeMap<u8, ProcessedWaveform>> = BTreeMap::new();

            for channel in &point.channels {
                for block in &channel.blocks {
                    for frame in &block.frames {
                        let entry = independent.entry(frame.time).or_default();
                        entry.insert(channel.id as u8, process_waveform(frame));
                    }
                }
            }

            let algorithm = Algorithm::default();

            let deltas = independent
                .iter()
                .collect::<Vec<_>>()
                .windows(2)
                .filter_map(|pair| {
                    let (time_1, waveforms) = pair[0];

                    if !(waveforms.len() == 1 && waveforms.contains_key(&5)) {
                        return None;
                    }

                    let events = waveform_to_events(&waveforms[&5], &algorithm);

                    if events.is_empty() {
                        None
                    } else {
                        // TODO: correct algorithm for multiple events
                        let amp = events[0].1;
                        let amp_kev = convert_to_kev(&amp, 5, &algorithm);

                        if !(range.contains(&amp_kev)) {
                            None
                        } else {
                            let (time_2, _) = pair[1];
                            Some((time_2 - time_1) as f32)
                        }
                    }
                })
                .collect::<Vec<_>>();

                histogram.lock().await.add_batch(0, deltas);
                pb.lock().await.inc(1);
        })
    }).collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(
            format!("(event within ({range:?} keV) -> next event) time deltas for {pattern}")
                .as_str(),
        ))
        .x_axis(Axis::new().title(Title::new("time delta, ns")))
        .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
        .height(1000);

    plot.set_layout(layout);
    
    
    histogram.try_lock().unwrap().draw_plotly(&mut plot, None);

    plot.show();
}
