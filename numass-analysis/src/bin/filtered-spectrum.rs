use std::{collections::BTreeMap, path::PathBuf};

use plotly::{common::Title, layout::Axis, Layout, Plot};

use processing::{
    histogram::PointHistogram,
    process::{convert_to_kev, process_waveform, waveform_to_events, Algorithm, StaticProcessParams}, 
    storage::load_point, 
    types::ProcessedWaveform
};

#[tokio::main]
async fn main() {
    

    // let filepath = "/data/numass-server/2022_12/Adiabacity_19_2/set_1/p4(200s)(HV1=15000)";
    // let range = 0.0..5.0;

    let filepath = "/data/numass-server/2022_12/Adiabacity_19_2/set_1/p5(200s)(HV1=14000)";
    let range = 0.0..6.0;

    // let filepath = "/data/numass-server/2022_12/Adiabacity_19_2/set_1/p6(200s)(HV1=13000)";
    // let range = 0.0..7.0;

    // let filepath = "/data/numass-server/2022_12/Adiabacity_19_2/set_1/p7(200s)(HV1=12000)";
    // let range = 0.0..8.0;

    let filepath = PathBuf::from(filepath);

    let mut independent: BTreeMap<u64, BTreeMap<u8, ProcessedWaveform>> = BTreeMap::new();

    let point = load_point(&filepath).await;
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

            let mut amps = vec![];

            for (ch, waveform) in waveforms {
                waveform_to_events(
                    waveform, *ch, 
                    &algorithm, &StaticProcessParams { baseline: None },
                    None
                ).iter().for_each(|(_, amp)| {
                    amps.push(convert_to_kev(amp, *ch, &algorithm));
                });
            }

            if !(amps.iter().any(|amp| range.contains(amp))) {
                None
            } else {
                let (time_2, waveforms_2) = pair[1];
                if (time_2 - time_1) > 8000 {
                    Some(amps)
                } else {
                    for (ch, waveform) in waveforms_2 {
                        waveform_to_events(
                            waveform, *ch, 
                            &algorithm, &StaticProcessParams { baseline: None },
                            None
                        ).iter().for_each(|(_, amp)| {
                            amps.push(convert_to_kev(amp, *ch, &algorithm));
                        });
                    }
                    Some(amps)
                }
            }
        })
        .flatten()
        .collect::<Vec<_>>();

    let mut histogram = PointHistogram::new_step(0.0..27.0, 0.1);
    histogram.add_batch(0, deltas.to_vec());

    let mut plot = Plot::new();

    let layout = Layout::new()
    .title(Title::new(format!("(event within ({range:?} keV) -> next event + time delta < 8 μs) spectrum for {filepath:?}").as_str()))
    .x_axis(Axis::new().title(Title::new("Amplitude, keV")))
    .height(1000);

    plot.set_layout(layout);
    histogram.draw_plotly(&mut plot, None);

    plot.show();
}
