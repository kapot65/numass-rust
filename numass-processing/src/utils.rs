//! # Utils
//! This module contains some utility functions not related to processing or postprocessing.

use std::collections::BTreeMap;

#[cfg(feature = "egui")] use {
    egui::{Color32, epaint::Hsva, plot::{Line, PlotUi}},
    crate::types::{ProcessedWaveform, RawWaveform}
};
#[cfg(feature = "plotly")]
use plotly::color::Color;

use crate::{constants::DETECTOR_BORDERS, histogram::{HistogramParams, PointHistogram}, types::NumassEvents};

pub fn events_to_histogram(
    amplitudes: NumassEvents, 
    histogram: HistogramParams
) -> PointHistogram {

    let mut histogram = PointHistogram::from(histogram);

    for (_, channels) in amplitudes {
        for (ch_num, (_, amp)) in channels {
            histogram.add(ch_num as u8, amp)
        }
    }

    histogram
}

/// Parabolic event amplitude correction correction
pub fn correct_amp(y0: f32, y1: f32, y2: f32) -> (f32, f32) {
    (
        // calculated with SymPy
        (y0 - y2) / (2.0 * (y0 - 2.0 * y1 + y2)),
        (-(y0 * y0) / 8.0 + y0 * y1 + y0 * y2 / 4.0 - 2.0 * y1 * y1 + y1 * y2 - (y2 * y2) / 8.0)
            / (y0 - 2.0 * y1 + y2),
    )
}


/// checks if frame triggered pixels is neighbors 
/// (frame with 3 or more triggers considered as neighbors due to its probability)
pub fn check_neigbors_fast<T>(frames: &BTreeMap<usize, T>) -> bool {

    let len = frames.len();
    
    match len {
        0 => false,
        1 => false,
        2 => {
            let [ch_1, ch_2] = {
                let mut keys = frames.keys();
                [*keys.next().unwrap() + 1, *keys.next().unwrap() + 1]
            };
            if ch_1 == 6 || ch_2 == 6 {
                true
            } else {
                let border = if ch_1 < ch_2 {
                    [ch_1, ch_2]
                } else {
                    [ch_2, ch_1]
                };
                !DETECTOR_BORDERS.contains(&border)
            }
        }
        _ => true
    }
}

#[cfg(feature = "egui")]
/// Returns color for channel index
/// Color will be same as [color_for_index_str](color_for_index_str)
pub fn color_for_index(idx: usize) -> Color32 {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;
    Hsva::new(h, 0.85, 0.5, 1.0).into()
}

#[cfg(feature = "plotly")]
/// Returns color for channel index
/// Color will be same as [color_for_index](color_for_index)
pub fn color_for_index_str(idx: usize) -> impl Color {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;

    let (r,g,b) = rgb_hsv::hsv_to_rgb((h, 0.85, 0.66));
    format!("rgb({r}, {g}, {b})")
}

#[cfg(feature = "egui")]
pub trait EguiLine: Into<Vec<[f64; 2]>> {
    fn draw_egui(self, plot_ui: &mut PlotUi, name: Option<&str>, color: Option<Color32>, thickness: Option<f32>, offset: Option<i64>) {
        let mut points: Vec<[f64; 2]> = self.into();
        if let Some(offset) = offset {
            points.iter_mut().for_each(|[x, _]| *x += offset as f64)
        }

        let mut line = Line::new(points);
        if let Some(color) = color {
            line = line.color(color)
        }
        if let Some(name) = name {
            line = line.name(name)
        }
        if let Some(thickness) = thickness {
            line = line.width(thickness)
        }

        plot_ui.line(line);
    }
}

#[cfg(feature = "egui")]
impl EguiLine for RawWaveform {}

#[cfg(feature = "egui")]
impl EguiLine for ProcessedWaveform {}