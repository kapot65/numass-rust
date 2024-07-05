use std::path::PathBuf;

use plotly::{common::Title, layout::Axis, Layout, Plot};

use processing::{
    histogram::PointHistogram,
    process::{ProcessParams, TRAPEZOID_DEFAULT},
    storage::load_point,
    types::FrameEvent,
};

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    let filepath = "/data-nvme/2024_03/Tritium_2/set_2/p174(30s)(HV1=11000)";
    let point = load_point(&PathBuf::from(filepath)).await;

    let frames = processing::process::extract_events(
        point,
        &ProcessParams {
            algorithm: TRAPEZOID_DEFAULT,
            convert_to_kev: true,
        },
    );
    let mut hist = PointHistogram::new(0.0..627.0, 627);

    frames.into_iter().for_each(|(_, events)| {
        events.into_iter().for_each(|(offset, event)| {
            if let FrameEvent::Reset { .. } = event {
                hist.add(0, (offset / 8) as f32);
            }
        })
    });

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(format!("resets distribution {filepath}").as_str()))
        .x_axis(Axis::new().title(Title::new("position, bins")))
        .height(1000);

    plot.set_layout(layout);
    hist.draw_plotly(&mut plot, None);
    plot.show();
}
