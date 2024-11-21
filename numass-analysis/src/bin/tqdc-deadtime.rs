use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use plotly::{common::Title, layout::Axis, Layout, Plot};

use processing::{
    histogram::PointHistogram, process::{extract_waveforms, frame_to_events, StaticProcessParams, TRAPEZOID_DEFAULT}, storage::load_point, types::FrameEvent
};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {

    let mut points = BTreeMap::new();

    points.insert(12000u16, vec![
        PathBuf::from("/data-fast/numass-server/2024_11/Tritium_2/set_2/p164(30s)(HV1=11450)"),
    ]);

    // points.insert(12000u16, vec![
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_2/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_3/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_4/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_5/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_7/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_2/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_3/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_4/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_5/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_6/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_7/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_8/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_4/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_4/set_2/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_5/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_5/set_2/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_6/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_6/set_2/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_7/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_7/set_2/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_8/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_8/set_2/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_9/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_9/set_2/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_10/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_10/set_2/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_11/set_1/p173(30s)(HV1=11050)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_11/set_2/p173(30s)(HV1=11050)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_12/set_1/p173(30s)(HV1=11050)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_12/set_2/p173(30s)(HV1=11050)"),
    // ]);

    // let mut points = BTreeMap::new();
    // points.insert(12000u16, vec![
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_2/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_3/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_4/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_5/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_2/set_7/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_2/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_3/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_4/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_5/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_6/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_7/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_3/set_8/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_4/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_4/set_2/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_5/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_5/set_2/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_6/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_6/set_2/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_7/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_7/set_2/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_8/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_8/set_2/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_9/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_9/set_2/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_10/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_10/set_2/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_11/set_1/p174(30s)(HV1=11000)"),
    //     // PathBuf::from("/data/numass-server/2024_03/Tritium_11/set_2/p174(30s)(HV1=11000)"),

    //     PathBuf::from("/data/numass-server/2024_03/Tritium_12/set_1/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_12/set_2/p174(30s)(HV1=11000)"),
    // ]);


    // let mut points = BTreeMap::new();
    // points.insert(12000u16, vec![
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_4/set_3/p174(30s)(HV1=11000)"),
    //     PathBuf::from("/data/numass-server/2024_03/Tritium_4/set_4/p174(30s)(HV1=11000)"),
    // ]);

    let hist = Arc::new(
        Mutex::new(PointHistogram::new_step(0.0..3e5, 24.0 * 4.0)));

    let handles = points.iter().flat_map(|(_, filepaths)| {
        filepaths.iter().map(|filepath| {
            let hist = Arc::clone(&hist);
            let filepath = filepath.clone();
            tokio::spawn(async move {

                let point = load_point(&filepath).await;

                let static_params = StaticProcessParams::from_point(&point);

                let frames = extract_waveforms(&point);

                let times = frames.into_iter().flat_map(|(time, frame)| {
                    
                    let mut frame_times = frame_to_events(
                        &frame, 
                        &TRAPEZOID_DEFAULT, 
                        &static_params, 
                        &mut None
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
            .title(Title::new("Time Deltas for Tritium_2/set_[123]/p174(30s)(HV1=11000)"))
            .x_axis(Axis::new().title(Title::new("time delta, ns")))
            .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
            .height(1000);
    
        plot.set_layout(layout);
        hist.draw_plotly(&mut plot, None);
        plot.show();

    }
    
}
