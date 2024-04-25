use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use plotly::{common::Title, layout::Axis, Layout, Plot};

use processing::{
    histogram::PointHistogram, process::{extract_waveforms, frame_to_events, StaticProcessParams, TRAPEZOID_DEFAULT}, storage::load_point, types::FrameEvent
};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {

    let mut points = BTreeMap::new();
    points.insert(12000u16, vec![PathBuf::from("/data-2/numass-server/2024_03/Tritium_7/set_1/p45(30s)(HV1=14000)")]);

    let hist = Arc::new(
        Mutex::new(PointHistogram::new_step(0.0..3e5, 24.0 * 4.0)));

    let handles = points.iter().flat_map(|(_, filepaths)| {
        filepaths.iter().map(|filepath| {
            let hist = Arc::clone(&hist);
            let filepath = filepath.clone();
            tokio::spawn(async move {

                let point = load_point(&filepath).await;

                let frames = extract_waveforms(&point);

                let times = frames.into_iter().flat_map(|(time, frame)| {
                    
                    let mut frame_times = frame_to_events(
                        &frame, 
                        &TRAPEZOID_DEFAULT, 
                        &StaticProcessParams { baseline: None }, 
                        None
                    ).into_iter().filter_map(|(ev_time, event)| {
                        if let FrameEvent::Event { .. } = event {
                            Some(time + ev_time as u64)
                        } else {
                            None
                        }
                    }).collect::<Vec<_>>();

                    frame_times.sort();
                    frame_times

                }).collect::<Vec<_>>();

                let deltas = {
                    let mut deltas = vec![0; times.len() - 1];
                    for idx in 1..times.len() {
                        deltas[idx - 1] = times[idx] - times[idx - 1]
                    }
                    let deltas = deltas
                        .iter()
                        .filter(|delta| **delta != 0)
                        .copied()
                        .collect::<Vec<_>>();
                    deltas
                };
                
                hist.lock().await.add_batch(0, 
                    deltas.iter().map(|x| *x as f32).collect::<Vec<_>>()
                );
            })
        })
    }).collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }
    

    {
        let hist = hist.lock().await;

        println!("delta\tcounts");
        for (idx, x) in hist.x.iter().enumerate() {
            println!("{x}\t{}", hist.channels[&0][idx]);
        }

        let mut plot = Plot::new();

        let layout = Layout::new()
            .title(Title::new("Time Deltas for /data-2/numass-server/2024_03/Tritium_7/set_1/p45(30s)(HV1=14000)"))
            .x_axis(Axis::new().title(Title::new("time delta, ns")))
            .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
            .height(1000);
    
        plot.set_layout(layout);
        hist.draw_plotly(&mut plot, None);
        plot.show();

    }
    
}
