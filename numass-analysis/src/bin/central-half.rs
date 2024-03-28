use std::path::PathBuf;

#[tokio::main]
async fn main() {
    use {
        plotly::Plot,
        processing::{
            histogram::PointHistogram,
            process::{convert_to_kev, process_waveform, waveform_to_events, Algorithm}, 
            storage::load_point
        },
        std::collections::BTreeMap,
    };

    // let filepath = "/data/numass-server/2022_12/Tritium_7/set_1/p7(30s)(HV1=18000)";
    // let filepath = "/data/numass-server/2022_12/Tritium_7/set_1/p5(30s)(HV1=18200)"; // substract with overflow!
    let filepath = "/data/numass-server/2022_12/Tritium_7/set_1/p98(30s)(HV1=13000)";

    let point = load_point(&PathBuf::from(filepath)).await;

    let mut crosses = BTreeMap::new();

    let algorithm = Algorithm::default();

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry: &mut Vec<_> = crosses.entry(frame.time).or_default();
                let waveform = process_waveform(frame);
                waveform_to_events(&waveform, &algorithm).iter().for_each(|(_, amp)| {
                    entry.push((channel.id as u8, convert_to_kev(amp, channel.id as u8, &algorithm)));
                });
            }
        }
    }

    let mut hist = PointHistogram::new(0.0..27.0, 270);

    let central = crosses
        .iter()
        .filter(|(_, waveforms)| waveforms.len() <= 2 && waveforms.iter().any(|(ch, _)| *ch == 5))
        .map(|(_, waveforms)| {
            waveforms
                .iter()
                .filter(|(ch, _)| *ch == 4 || *ch == 2 || *ch == 6 || *ch == 5)
                .map(|(_, amp)| *amp)
                .sum::<f32>()
        })
        .collect::<Vec<_>>();

    let second = crosses
        .iter()
        .filter(|(_, waveforms)| waveforms.len() <= 2 && waveforms.iter().any(|(ch, _)| *ch == 1))
        .map(|(_, waveforms)| {
            waveforms
                .iter()
                .filter(|(ch, _)| *ch == 0 || *ch == 3 || *ch == 5 || *ch == 1)
                .map(|(_, amp)| *amp)
                .sum::<f32>()
        })
        .collect::<Vec<_>>();

    hist.add_batch(5, central);
    hist.add_batch(1, second);

    let mut plot = Plot::new();

    let layout = plotly::Layout::new().height(1000);
    plot.set_layout(layout);
    hist.draw_plotly_each_channel(&mut plot);

    plot.show()
}
