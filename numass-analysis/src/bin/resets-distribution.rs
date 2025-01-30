//! Построение распределений сбросов [processing::types::FrameEvent::Reset] в кадре
use std::path::PathBuf;

use plotly::{common::Title, layout::Axis, Layout, Plot};

use processing::{
    histogram::PointHistogram,
    process::{ProcessParams, TRAPEZOID_DEFAULT},
    storage::{load_meta, load_point},
    types::FrameEvent,
};

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[tokio::main]
async fn main() {
    let filepath = "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p196(20s)(HV1=10000)";

    let filepath = PathBuf::from(filepath);
    
    let meta = load_meta(&filepath).await;
    let point = load_point(&filepath).await;
    

    let (frames, _) = processing::process::extract_events(
        meta,
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
        .title(Title::new(format!("resets distribution {filepath:?}").as_str()))
        .x_axis(Axis::new().title(Title::new("position, bins")))
        .height(1000);

    plot.set_layout(layout);
    hist.draw_plotly(&mut plot, None);
    plot.show();
}
