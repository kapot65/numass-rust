//! Построение спектра амплитуд для одной точки
//! Используется для визуализации определения базовой линии

use std::path::PathBuf;

use plotly::Plot;
use processing::{preprocess::point_to_amp_hist, process::TRAPEZOID_DEFAULT, storage::load_point};

#[tokio::main]
async fn main() {

    // let path = "/data-fast/numass-server/2024_11/Tritium_3/set_2/p0(30s)(HV1=14000)";
    let path = "/data-fast/numass-server/2024_11/Tritium_3/set_1/p196(20s)(HV1=10000)";
    let point = load_point(&PathBuf::from(path)).await;
    
    let amps = point_to_amp_hist(&point, &TRAPEZOID_DEFAULT);

    let mut plot = Plot::new();

    amps.draw_plotly_each_channel(&mut plot);

    let layout = plotly::Layout::new()
        .title(plotly::common::Title::new(&format!("Amp Spectrum for baselines for {path}")))
        .height(1000);
    plot.set_layout(layout);
    plot.show();
}
