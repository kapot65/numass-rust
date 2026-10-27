//! # tqdc-deadtime
//! Скрипт рисует гистограмму времен между ближайшими событиями в файле данных.
//!
//!
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use plotly::{Layout, Plot, common::Title, layout::Axis};

use numass_processing::{
    histogram::PointHistogram,
    preprocess::{PreprocessParams, apply_waveforms, extract_waveforms},
    process::{TRAPEZOID_DEFAULT, extract_frame},
    storage::load_point,
    types::FrameEvent,
};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    let point_path = "/data-fast/numass-server/2024_11/Tritium_2_1/set_4/p196(20s)(HV1=10000)";
    let precise = true;
    // true - считает дельты между событиями, false - между триггерами кадров
    let hist = Arc::new(Mutex::new(PointHistogram::new_step(0.0..3e5, 24.0 * 1.0)));
    let mut points = BTreeMap::new();
    points.insert(12000u16, vec![PathBuf::from(point_path)]);

    let handles = points
        .values()
        .flat_map(|filepaths| {
            filepaths.iter().map(|filepath| {
                let hist = Arc::clone(&hist);
                let filepath = filepath.clone();
                tokio::spawn(async move {
                    let point = load_point(&filepath).await.expect("load point");

                    let frames = extract_waveforms(&point);
                    let processed = apply_waveforms(&frames, &PreprocessParams::default().steps)
                        .expect("preprocess");

                    let times = frames
                        .into_iter()
                        .flat_map(|(time, frame)| {
                            let mut frame_times = extract_frame(
                                &processed[&time],
                                &frame,
                                &TRAPEZOID_DEFAULT,
                                &mut None,
                            )
                            .expect("process")
                            .into_iter()
                            .filter_map(|(_ev_time, event)| {
                                if let FrameEvent::Event { .. } = event {
                                    if precise {
                                        Some(time + _ev_time as u64)
                                    } else {
                                        Some(time)
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect::<HashSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>();

                            frame_times.sort();
                            frame_times
                        })
                        .collect::<Vec<_>>();

                    let deltas = {
                        let mut deltas = vec![0; times.len() - 1];
                        for idx in 1..times.len() {
                            deltas[idx - 1] = times[idx] - times[idx - 1]
                        }
                        deltas
                            .iter()
                            .filter(|delta| **delta != 0)
                            .copied()
                            .collect::<Vec<_>>()
                    };

                    hist.lock()
                        .await
                        .add_batch(0, deltas.iter().map(|x| *x as f32).collect::<Vec<_>>());
                })
            })
        })
        .collect::<Vec<_>>();

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
            .title(Title::new(&format!("Time Deltas for {point_path}")))
            .x_axis(Axis::new().title(Title::new("time delta, ns")))
            .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
            .height(1000);

        plot.set_layout(layout);
        hist.draw_plotly(&mut plot, None);
        plot.show();
    }
}
