use std::collections::BTreeMap;

use protobuf::Message;

use processing::{
    histogram::PointHistogram,  numass::{protos::rsb_event, NumassMeta},
    convert_to_kev, waveform_to_events, process_waveform, Algorithm
};
use dataforge::read_df_message;

#[tokio::main]
async fn main() {

    let files = [
        "/data/numass-server/2022_12/Tritium_7/set_1/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_2/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_3/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_4/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_5/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_6/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_7/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_8/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_9/p52(30s)(HV1=15000)",
        "/data/numass-server/2022_12/Tritium_7/set_10/p52(30s)(HV1=15000)",
    ];

    // let files = [
    //     "/data/numass-server/2022_12/Tritium_7/set_1/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_2/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_3/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_4/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_5/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_6/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_7/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_8/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_9/p64(30s)(HV1=14500)",
    //     "/data/numass-server/2022_12/Tritium_7/set_10/p64(30s)(HV1=14500)",
    // ];

    // let monitor_indices = [30, 39, 47, 53, 58, 66, 75, 76, 85, 93, 103, 111, 121];
    // let mut files = vec![];
    // for set_number in 1..=10 {
    //     for point_idx in monitor_indices {
    //         files.push(format!("/data/numass-server/2022_12/Tritium_7/set_{set_number}/p{point_idx}(30s)(HV1=14000)"))
    //     }
    // }

    // let files = [
    //     "/data/numass-server/2022_12/Tritium_7/set_1/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_2/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_3/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_4/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_5/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_6/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_7/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_8/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_9/p98(30s)(HV1=13000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_10/p98(30s)(HV1=13000)",
    // ];

    // let files = [
    //     "/data/numass-server/2022_12/Tritium_7/set_1/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_2/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_3/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_4/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_5/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_6/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_7/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_8/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_9/p120(30s)(HV1=12000)",
    //     "/data/numass-server/2022_12/Tritium_7/set_10/p120(30s)(HV1=12000)",
    // ];

    let crosses = {
        let mut crosses = BTreeMap::new();

        let handles = files
            .iter()
            .map(|filepath| {
                let filepath = filepath.to_owned();
                tokio::spawn(async move {
                    let mut crosses = BTreeMap::new();

                    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
                    let message = read_df_message::<NumassMeta>(&mut point_file)
                        .await
                        .unwrap();

                    let point =
                        rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                    for channel in &point.channels {
                        for block in &channel.blocks {
                            for frame in &block.frames {
                                let entry: &mut BTreeMap<_, _> = crosses.entry(frame.time).or_default();
                                entry.insert(channel.id as usize, process_waveform(frame));
                            }
                        }
                    }

                    let filtered_crosses = crosses
                        .iter()
                        .filter(|(_, waveforms)| waveforms.len() > 1)
                        .map(|(ch_num, waveform)| (*ch_num, waveform.to_owned()))
                        .collect::<BTreeMap<_, _>>();

                    filtered_crosses
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            let mut crosses_local = handle.await.unwrap();
            // println!("{counts_local:?}");
            crosses.append(&mut crosses_local);
        }
        crosses
    };

    let double_non_crosses = crosses
        .iter()
        .filter(|(_, waveforms)| waveforms.len() == 2)
        .filter(|(_, waveforms)| {
            processing::check_neigbors_fast(waveforms)
        });


    let algorithm = Algorithm::default();
    let non_crosses_amps = double_non_crosses
        .map(|(_, waveforms)| {
            waveforms
                .iter()
                .map(|(ch_id, waveform)| {
                    waveform_to_events(waveform, &algorithm).iter().map(|(_, amp)| {
                        convert_to_kev(amp, *ch_id as u8, &algorithm)
                    }).sum::<f32>()
                })
                .sum::<f32>()
        })
        .collect::<Vec<_>>();

    let mut histogram = PointHistogram::new_step(0.0..50.0, 0.1);
    histogram.add_batch(0, non_crosses_amps);

    let mut plot = plotly::Plot::new();

    let layout = plotly::Layout::new()
        .title(plotly::common::Title::new("non-crosses"))
        .x_axis(plotly::layout::Axis::new().title(plotly::common::Title::new("time delta, ns")))
        .height(1000);

    plot.set_layout(layout);
    histogram.draw_plotly(&mut plot, None);

    plot.show();
}
