use std::sync::Arc;

use plotly::{common::Title, layout::Axis, Layout, Plot};
use protobuf::Message;

use dataforge::read_df_message;
use processing::{
    process_waveform, histogram::PointHistogram, numass::{protos::rsb_event, NumassMeta}
};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    
    let db_root = "/data-nvme";
    let run = "2023_03";

    // let filepath = "/data/numass-server/2023_03/Tritium_1/set_1/p118(30s)(HV1=12000)";
    // let filepath = "/data/2022_12/Tritium_7/set_1/p120(30s)(HV1=12000)";
    // let filepath = "/data/2022_12/Tritium_7/set_1/p0(30s)(HV1=14000)";
    // let filepath = "/data/2022_12/Tritium_7/set_1/p6(30s)(HV1=18100)";
    let pattern = format!("/{run}/Tritium_2/set_1/p*(HV1=12000)");
    // let pattern = format!("/{run}/Tritium_3/set_*/p*(HV1=15000)");
    let exclude: Vec<String> = vec![
        "Tritium_3/set_25_short".to_owned(),
        "Tritium_2/set_29/p37".to_owned()
    ];

    let points = analysis::get_points_by_pattern(db_root, &pattern, &exclude);

    let hist = Arc::new(
        Mutex::new(PointHistogram::new_step(0.0..3e5, 24.0 * 4.0)));

    let handles = points.iter().flat_map(|(_, filepaths)| {
        filepaths.iter().map(|filepath| {
            let hist = Arc::clone(&hist);
            let filepath = filepath.clone();
            tokio::spawn(async move {

                let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
                let message = read_df_message::<NumassMeta>(&mut point_file)
                    .await
                    .unwrap();

                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

                let mut times = point
                    .channels
                    .iter()
                    .flat_map(|channel| {
                        channel.blocks.iter().flat_map(|block| {
                            block.frames.iter().filter_map(|frame| {

                                // Some(frame.time)

                                let waveform = process_waveform(frame);
                                let threshold = 100.0;

                                processing::find_first_peak(&waveform, threshold).map(|x| {
                                    let x_offset = x as u64 * 8;
                                    frame.time + x_offset
                                })
                            })
                        })
                    })
                    .collect::<Vec<_>>();
                times.sort();

                let deltas = {
                    let mut deltas = vec![0; times.len() - 1];
                    for idx in 1..times.len() {
                        deltas[idx - 1] = times[idx] - times[idx - 1]
                    }
                    let mut deltas = deltas
                        .iter()
                        .filter(|delta| **delta != 0)
                        .copied()
                        .collect::<Vec<_>>();
                    deltas.sort();
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
        let mut plot = Plot::new();

        let layout = Layout::new()
            .title(Title::new(format!("Time Deltas for {pattern}").as_str()))
            .x_axis(Axis::new().title(Title::new("time delta, ns")))
            .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
            .height(1000);
    
        plot.set_layout(layout);
        hist.draw_plotly(&mut plot, None);
        plot.show();

    }
    
}
