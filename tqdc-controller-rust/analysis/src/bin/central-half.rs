#[tokio::main]
async fn main() {
    use {
        dataforge::read_df_message,
        numass::{protos::rsb_event, NumassMeta},
        plotly::{
            common::{Line, LineShape},
            Plot, Scatter,
        },
        processing::{
            convert_to_kev, frame_to_waveform, histogram::PointHistogram, waveform_to_event,
            Algorithm,
        },
        protobuf::Message,
        std::collections::BTreeMap,
    };

    // let filepath = "/data/numass-server/2022_12/Tritium_7/set_1/p7(30s)(HV1=18000)";
    // let filepath = "/data/numass-server/2022_12/Tritium_7/set_1/p5(30s)(HV1=18200)"; // substract with overflow!
    let filepath = "/data/numass-server/2022_12/Tritium_7/set_1/p98(30s)(HV1=13000)";

    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
    let message = read_df_message::<NumassMeta>(&mut point_file)
        .await
        .unwrap();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let mut crosses = BTreeMap::new();

    let algorithm = Algorithm::Likhovid { left: 6, right: 36 };

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry: &mut Vec<_> = crosses.entry(frame.time).or_default();

                let waveform = frame_to_waveform(frame);
                let amp = waveform_to_event(&waveform, &algorithm).1;
                let amp = convert_to_kev(&amp, channel.id as u8, &algorithm);

                entry.push((channel.id as u8, amp));
            }
        }
    }

    let mut hist = PointHistogram::new((0.0, 27.0), 270);

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

    let layout = plotly::Layout::new()
        // .title(plotly::common::Title::new("calibration data"))
        .height(1000);
    plot.set_layout(layout);

    let trace_ch_2 = Scatter::new(hist.x.clone(), hist.channels.get(&1).unwrap().clone())
        .name("ch 2")
        .line(Line::new().shape(LineShape::Hvh));

    let trace_ch_6 = Scatter::new(hist.x, hist.channels.get(&5).unwrap().clone())
        .name("ch 6")
        .line(Line::new().shape(LineShape::Hvh));

    plot.add_trace(trace_ch_6);
    plot.add_trace(trace_ch_2);
    plot.show()
}
