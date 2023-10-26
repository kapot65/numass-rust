use std::vec;

pub extern crate numass;
pub mod histogram;
pub mod viewer; // TODO: move to numass-processing with viewer feature

mod constants;

use histogram::HistogramParams;
use serde::{Deserialize, Serialize};
use {histogram::PointHistogram, numass::protos::rsb_event, std::collections::BTreeMap};
use constants::{KEV_COEFF_FIRST_PEAK, KEV_COEFF_LIKHOVID, KEV_COEFF_MAX};

#[cfg(feature = "egui")]
use egui::{Color32, epaint::Hsva, plot::{PlotUi, Line}};

#[cfg(feature = "egui")]
pub fn color_for_index(idx: usize) -> Color32 {
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;
    Hsva::new(h, 0.85, 0.5, 1.0).into()
}

#[cfg(feature = "plotly")]
use plotly::color::Color;

#[cfg(feature = "plotly")]
pub fn color_for_index_str(idx: usize) -> impl Color {
    
    let golden_ratio = (5.0_f32.sqrt() - 1.0) / 2.0; // 0.61803398875
    let h = idx as f32 * golden_ratio;

    let (r,g,b) = rgb_hsv::hsv_to_rgb((h, 0.85, 0.66));
    format!("rgb({r}, {g}, {b})")
}

const DETECTOR_BORDERS: [[usize; 2]; 8] = [
        [1, 3],
        [1, 4],
        [1, 7],
        [2, 3],
        [2, 5],
        [2, 7],
        [3, 4],
        [4, 5],
];

// checks if frame triggered pixels is neighbors 
// (frame with 3 or more triggers considered as neighbors due to its probability)
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


#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct ProcessParams {
    pub algorithm: Algorithm,
    pub convert_to_kev: bool,
}

impl Default for ProcessParams {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::default(),
            convert_to_kev: true,
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct PostProcessParams {
    pub merge_close_events: bool,
    pub merge_map: [[bool; 7]; 7],
    pub use_dead_time: bool,
    pub effective_dead_time: u64,
}

impl Default for PostProcessParams {
    fn default() -> Self {
        Self {
            merge_close_events: true,
            use_dead_time: false,
            effective_dead_time: 4000,
            merge_map: [
                [false, true, false, false, false, false, false],
                [false, false, false, true, false, false, false],
                [false, false, false, false, true, false, false],
                [false, false, false, false, false, false, true],
                [true, false, false, false, false, false, false],
                [true, true, true, true, true, false, true],
                [false, false, true, false, false, false, false],
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFrame {
    pub time: u64,
    pub waveforms: BTreeMap<u8, ProcessedWaveform>,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub enum Algorithm {
    Max,
    Likhovid { left: usize, right: usize },
    FirstPeak { threshold: i16, left: usize }
}

impl Default for Algorithm {
    fn default() -> Self {
        Self::FirstPeak { threshold: 10, left: 8 }
    }
}

pub fn extract_events(point: &rsb_event::Point, params: &ProcessParams) -> BTreeMap<u64, BTreeMap<usize, (u16, f32)>> {

    let mut amplitudes = BTreeMap::new();

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = amplitudes.entry(frame.time).or_insert(BTreeMap::new());

                let waveform = process_waveform(frame);

                for (time, amp) in waveform_to_events(&waveform, &params.algorithm) {
                    let amp = if params.convert_to_kev {
                        convert_to_kev(&amp, channel.id as u8, &params.algorithm)
                    } else {
                        amp
                    };
                    entry.insert(channel.id as usize, (time, amp));
                }
            }
        }
    }

    amplitudes
}

pub fn post_process(mut amplitudes: BTreeMap<u64, BTreeMap<usize, (u16, f32)>>, params: &PostProcessParams) -> BTreeMap<u64, BTreeMap<usize, (u16, f32)>> {

    let mut last_time: u64 = 0;
    amplitudes.iter_mut().filter_map(|(time, channels)| {

        if params.use_dead_time && last_time.abs_diff(*time) < params.effective_dead_time {
            return None;
        }

        last_time = *time;

        // TODO: add time analysis
        if params.merge_close_events {
            for ch_1 in 0..7 {
                for ch_2 in 0..7 {
                    if params.merge_map[ch_1][ch_2]
                        && channels.contains_key(&ch_1)
                        && channels.contains_key(&ch_2)
                    {
                        let amp2 = channels.get(&ch_2).unwrap().to_owned().1;
                        channels.entry(ch_1).and_modify(|(_, amp)| *amp += amp2);
                        channels.remove_entry(&ch_2).unwrap();
                    }
                }
            }
        }

        Some((*time, channels.clone()))
    }).collect::<BTreeMap<_,_>>()
    
}

pub fn events_to_histogram(
        amplitudes: BTreeMap<u64, BTreeMap<usize, (u16, f32)>>, 
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

// TODO: add channel id
#[derive(Debug, Clone)]
pub struct RawWaveform(pub Vec<i16>);

impl RawWaveform {
    pub fn to_egui_line(&self, offset: i64) -> Vec<[f64; 2]> {
        self.0.iter()
            .enumerate()
            .map(|(x, y)| [(x as i64 + offset) as f64, (*y as f64)])
            .collect::<Vec<_>>()
    }
}

impl From<RawWaveform> for Vec<[f64; 2]> {
    fn from(waveform: RawWaveform) -> Self {
        waveform.0.iter()
            .enumerate()
            .map(|(x, y)| [x as f64, *y as f64])
            .collect::<Vec<_>>()
    }
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

impl From<Vec<i16>> for RawWaveform {
    fn from(data: Vec<i16>) -> Self {
        Self(data)
    }
}

impl From<rsb_event::point::channel::block::Frame> for RawWaveform {
    fn from(frame: rsb_event::point::channel::block::Frame) -> Self {
        let waveform_len = frame.data.len() / 2;
        RawWaveform((0..waveform_len)
        .map(|idx| i16::from_le_bytes(frame.data[idx * 2..idx * 2 + 2].try_into().unwrap()))
        .collect::<Vec<_>>())
    }
}

impl From<&rsb_event::point::channel::block::Frame> for RawWaveform {
    fn from(frame: &rsb_event::point::channel::block::Frame) -> Self {
        let waveform_len = frame.data.len() / 2;
        RawWaveform((0..waveform_len)
        .map(|idx| i16::from_le_bytes(frame.data[idx * 2..idx * 2 + 2].try_into().unwrap()))
        .collect::<Vec<_>>())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedWaveform(pub Vec<f32>);

impl From<ProcessedWaveform> for Vec<[f64; 2]> {
    fn from(waveform: ProcessedWaveform) -> Self {
        waveform.0.iter()
            .enumerate()
            .map(|(x, y)| [x as f64, *y as f64])
            .collect::<Vec<_>>()
    }
}

// TODO: add static correction
pub fn process_waveform(waveform: impl Into<RawWaveform>) -> ProcessedWaveform {
    let waveform = waveform.into();
    // let baseline = 0.0;
    let baseline = waveform.0.iter().take(16).sum::<i16>() as f32 / 16.0;
    ProcessedWaveform(waveform.0.iter().map(|bin| *bin as f32 - baseline).collect::<Vec<_>>())
}

pub fn convert_to_kev(amplitude: &f32, ch_id: u8, algorithm: &Algorithm) -> f32 {
    match algorithm {
        Algorithm::Max => {
            let [a, b] = KEV_COEFF_MAX[ch_id as usize];
            a * *amplitude + b
        }
        Algorithm::Likhovid { .. } => {
            let [a, b] = KEV_COEFF_LIKHOVID[ch_id as usize];
            a * *amplitude + b
        }
        Algorithm::FirstPeak { .. } => {
            
            let [a, b] = KEV_COEFF_FIRST_PEAK[ch_id as usize];
            a * *amplitude + b
        }
    }
}

pub fn waveform_to_events(waveform: &ProcessedWaveform, algorithm: &Algorithm) -> Vec<(u16, f32)> {
    let (x, y) = waveform.0
        .iter()
        .enumerate()
        .max_by(|first, second| {
            first.1.partial_cmp(second.1).unwrap()
        })
        .unwrap();

    match algorithm {
        Algorithm::Max => vec![(x as u16 * 8, *y)],
        Algorithm::Likhovid { left, right } => {
            let amplitude = {
                let left = if x >= *left { x - left } else { 0 };
                let right = std::cmp::min(waveform.0.len(), x + right);
                let crop = &waveform.0[left..right];
                crop.iter().sum::<f32>() / crop.len() as f32
            };

            vec![(x as u16 * 8, amplitude)]
        }
        Algorithm::FirstPeak { threshold, left } => {
            let pos = find_first_peak(waveform, *threshold as f32);
            if let Some(pos) = pos {
                let left = if &pos < left {
                    0
                } else {
                    pos - left
                };
                // let length = (waveform.0.len() - pos) as f32;
                let amplitude = waveform.0[left..waveform.0.len()].iter().sum::<f32>();
                vec![(pos as u16 * 8, amplitude / 50.0)]
            } else {
                vec![]
            }
        }
    }
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

pub fn find_first_peak(waveform: &ProcessedWaveform, threshold: f32) -> Option<usize> {
    waveform.0
        .iter()
        .enumerate()
        .find(|(idx, amp)| {
            let amp = **amp;
            amp > threshold
                && (*idx == 0 || waveform.0[idx - 1] <= amp)
                && (*idx == waveform.0.len() - 1 || waveform.0[idx + 1] <= amp)
        })
        .map(|(idx, _)| idx)
}
