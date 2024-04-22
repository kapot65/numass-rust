use std::path::PathBuf;

use plotly::{Plot, Layout, common::Title, layout::Axis};
use processing::{histogram::PointHistogram, process::ProcessParams, storage::process_point};

#[tokio::main]
async fn main() {
    
    let filepath = PathBuf::from("/data-nvme/2023_03/Tritium_5/set_1/p118(30s)(HV1=12000)");
                                          

    let events = process_point(&filepath, &ProcessParams::default()).await.unwrap().1.unwrap();

    let mut histogram = PointHistogram::new_step(0.0..31.0, 0.1);

    events.into_iter().for_each(|(time, block)| {
        let time_s = time as f32 * 1e-9;
        block.into_iter().for_each(|(ch_num, _)| {
            histogram.add(ch_num as u8, time_s);
        });
    });

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(format!("Precise count rate for {filepath:?}").as_str()))
        .x_axis(Axis::new().title(Title::new("time, ns")))
        // .y_axis(Axis::new().type_(plotly::layout::AxisType::Log))
        .height(1000);

    plot.set_layout(layout);
    histogram.draw_plotly(&mut plot, None);

    // println!("{}", plot.to_json());
    plot.show();
}