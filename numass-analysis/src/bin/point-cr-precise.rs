use dataforge::read_df_message_sync;
use plotly::{Plot, Layout, common::Title, layout::Axis};
use processing::{numass::{protos::rsb_event, NumassMeta}, histogram::PointHistogram, ProcessParams, extract_events};
use protobuf::Message;

fn main() {
    
    let filepath = "/data-nvme/2023_03/Tritium_5/set_1/p118(30s)(HV1=12000)";
                                          
    let point = {
        let mut point_file = std::fs::File::open(filepath).unwrap();
        let message = read_df_message_sync::<NumassMeta>(&mut point_file).unwrap();
        rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap()
    };

    let params = ProcessParams::default();
    let amplitudes = extract_events(&point, &params);

    let mut histogram = PointHistogram::new_step(0.0..31.0, 0.1);

    amplitudes.into_iter().for_each(|(time, block)| {
        let time_s = time as f32 * 1e-9;
        block.into_iter().for_each(|(ch_num, _)| {
            histogram.add(ch_num as u8, time_s);
        });
    });

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(format!("Precise count rate for {filepath}").as_str()))
        .x_axis(Axis::new().title(Title::new("time, ns")))
        // .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
        .height(1000);

    plot.set_layout(layout);
    histogram.draw_plotly(&mut plot, None);

    // println!("{}", plot.to_json());
    plot.show();
}