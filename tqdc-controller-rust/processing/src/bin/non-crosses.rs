use std::collections::HashMap;

use processing::{waveform_to_event, Algorithm, convert_to_kev};
use protobuf::Message;

use dataforge::read_df_message;
use numass::{protos::rsb_event, NumassMeta};

#[tokio::main]
async fn main() {

    // let files = [
    //     "/data/2022_12/Tritium_7/set_1/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_2/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_3/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_4/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_5/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_6/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_7/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_8/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_9/p52(30s)(HV1=15000)",
    //     "/data/2022_12/Tritium_7/set_10/p52(30s)(HV1=15000)",
    // ];

    // let files = [
    //     "/data/2022_12/Tritium_7/set_1/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_2/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_3/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_4/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_5/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_6/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_7/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_8/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_9/p64(30s)(HV1=14500)",
    //     "/data/2022_12/Tritium_7/set_10/p64(30s)(HV1=14500)",
    // ];

    let monitor_indices = [30,39,47,53,58,66,75,76,85,93,103,111,121];
    let mut files = vec![];
    for set_number in 1..=10 {
        for point_idx in monitor_indices {
            files.push(format!("/data/2022_12/Tritium_7/set_{set_number}/p{point_idx}(30s)(HV1=14000)"))
        }
    }

    // let files = [
    //     "/data/2022_12/Tritium_7/set_1/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_2/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_3/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_4/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_5/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_6/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_7/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_8/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_9/p98(30s)(HV1=13000)",
    //     "/data/2022_12/Tritium_7/set_10/p98(30s)(HV1=13000)",
    // ];

    // let files = [
    //     "/data/2022_12/Tritium_7/set_1/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_2/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_3/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_4/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_5/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_6/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_7/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_8/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_9/p120(30s)(HV1=12000)",
    //     "/data/2022_12/Tritium_7/set_10/p120(30s)(HV1=12000)",
    // ];


    let crosses = {

        let mut crosses = vec![];

        let handles = files.iter().map(|filepath| {

            let filepath = filepath.to_owned();
            tokio::spawn(async move {
                let mut crosses = HashMap::new();

                let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
                let message = read_df_message::<NumassMeta>(&mut point_file).await.unwrap();

                let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                for channel in &point.channels {
                    for block in &channel.blocks {
                        for frame in &block.frames {
                            let entry: &mut Vec<_> = crosses.entry(frame.time).or_default();
                            entry.push((channel.id as u8, processing::frame_to_waveform(frame)));
                        }
                    }
                }

                let filtered_crosses = crosses.iter()
                .filter(|(_, waveforms)| { waveforms.len() > 1 })
                .filter(|(_, waveforms)| {

                    let mut has_duplicate = false;

                    for idx_1 in 0..waveforms.len() - 1 {
                        for idx_2 in idx_1 + 1..waveforms.len() {
                            if waveforms[idx_1].0 == waveforms[idx_2].0 {
                                has_duplicate = true;
                                break;
                            }
                        }
                    }

                    !has_duplicate
                })
                .map(|(ch_num, waveform)| (*ch_num, waveform.to_owned())).collect::<Vec<_>>();

                filtered_crosses

            })
        }).collect::<Vec<_>>();

        for handle in handles {
            let mut crosses_local = handle.await.unwrap();
            // println!("{counts_local:?}");
            crosses.append(&mut crosses_local);
        }
        crosses
    };

    let borders = [
        [1u8, 3],
        [1, 4],
        [1, 7],
        [2, 3],
        [2, 5],
        [2, 7],
        [3, 4],
        [4, 5],
    ];

    let double_non_crosses = crosses.iter()
        .filter(|(_, waveforms)| waveforms.len() == 2)
        .filter(|(_, waveforms)| {
            let (ch_1, ch_2) = (waveforms[0].0 + 1, waveforms[1].0 + 1);
            let border = if ch_1 < ch_2 { [ch_1, ch_2 ] } else { [ch_2, ch_1] };
            borders.contains(&border)
        });

    println!("{:?}", double_non_crosses.clone().map(|(_, waveforms)| {
        (waveforms[0].0 + 1, waveforms[1].0 + 1)
    }).collect::<Vec<_>>());


    let algorithm = Algorithm::Likhovid { left: 6, right: 36 };
    let non_crosses_amps = double_non_crosses.map(|(_, waveforms)| {
        waveforms.iter().map(|(ch_id, waveform)| {
            convert_to_kev(&waveform_to_event(waveform, &algorithm).1, *ch_id, &algorithm)
        }).sum::<f32>()
    }).collect::<Vec<_>>();

    let trace2 = plotly::Histogram::new(non_crosses_amps)
        .x_bins(plotly::histogram::Bins::new(0.0, 50.0, 0.1))
        .opacity(0.6);

    let mut plot = plotly::Plot::new();

    let layout = plotly::Layout::new()
    .title(plotly::common::Title::new("non-crosses"))
    .x_axis(plotly::layout::Axis::new().title(plotly::common::Title::new("time delta, ns")))
    // .y_axis(plotly::layout::Axis::new().type_(plotly::layout::AxisType::Log))
    
    .height(1000);
    plot.set_layout(layout);
    plot.add_trace(trace2);

    plot.show();
}