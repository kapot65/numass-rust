use std::path::PathBuf;

use plotly::Plot;
use processing::{process::{point_to_amp_hist, TRAPEZOID_DEFAULT}, storage::load_point};



#[tokio::main]
async fn main() {

    let filepath = PathBuf::from("/data-fast/numass-server/2024_11/Tritium_2/set_3/p0(30s)(HV1=14000)");

    let point = load_point(&filepath).await;
    let algo = TRAPEZOID_DEFAULT;

    let hist = point_to_amp_hist(&point, &algo);

    let mut plot = Plot::new();

    // let layout = Layout::new()
    //     .title(Title::new(format!("resets distribution {filepath}").as_str()))
    //     .x_axis(Axis::new().title(Title::new("position, bins")))
    //     .height(1000);

    // plot.set_layout(layout);
    hist.draw_plotly_each_channel(&mut plot);

    plot.show();
}