use std::vec;

use histogram::HistogramParams;
use serde::{Deserialize, Serialize};
pub extern crate numass;
use {histogram::PointHistogram, numass::protos::rsb_event, std::collections::BTreeMap};
pub mod histogram;

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ProcessingParams {
    pub algorithm: Algorithm,
    pub post_processing: PostProcessingParams,
    pub histogram: HistogramParams,
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct PostProcessingParams {
    pub convert_to_kev: bool,
    pub merge_close_events: bool,
    pub merge_map: [[bool; 7]; 7],
    pub use_dead_time: bool,
    pub effective_dead_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFrame {
    pub time: u64,
    pub waveforms: BTreeMap<u8, ProcessedWaveform>,
}

impl Default for ProcessingParams {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::default(),
            post_processing: PostProcessingParams {
                // TODO: add to KeV corrections
                convert_to_kev: true,
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
            },
            histogram: HistogramParams { range: 0.0..27.0, bins: 270 }
        }
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Algorithm {
    Max,
    Likhovid { left: usize, right: usize },
    FirstPeak { threshold: i16, left: usize }
}

impl Default for Algorithm {
    fn default() -> Self {
        Self::Likhovid { left: 15, right: 36 }
    }
}

// TODO remove hardcode
const KEV_COEFF_MAX: [[f32; 2]; 7] = [
    [0.059379287, 0.31509972],
    [0.060557768, 0.26772976],
    [0.06317734, 0.23027992],
    [0.062333938, 0.26050186],
    [0.062186483, 0.25954437],
    [0.06751788, 0.2222414],
    [0.05806803, 0.14519024],
];

// coeffs for (3,19)
// const KEV_COEFF_LIKHOVID: [[f32; 2]; 7] = [
//     [0.134678, 0.09647 ],
//     [0.141536, 0.060275],
//     [0.147718, 0.027412],
//     [0.150288, 0.038774],
//     [0.15131 , 0.071923],
//     [0.15336 , 0.029206],
//     [0.136762, 0.041848]
// ];

const KEV_COEFF_LIKHOVID: [[f32; 2]; 7] = [
    [
        0.3175972,
        0.071510315,
    ],
    [
        0.2723175,
        0.08074951,
    ],
    [
        0.2869933,
        0.082289696,
    ],
    [
        0.29424095,
        -0.0075092316,
    ],
    [
        0.29598197,
        0.06416798,
    ],
    [
        0.2869933,
        0.082289696,
    ],
    [
        0.26007754,
        -0.017463684,
    ],
];

const KEV_COEFF_FIRST_PEAK: [[f32; 2]; 7] = [
    [
        0.30209273,
        0.058135986,
    ],
    [
        0.25891086,
        -0.0007972717,
    ],
    [
        0.2746626,
        -0.036146164,
    ],
    [
        0.27816013,
        0.050985336,
    ],
    [
        0.28441244,
        -0.08033466,
    ],
    [
        0.27044022,
        0.05974865,
    ],
    [
        0.2477852,
        -0.06184864,
    ],
];

pub fn extract_amplitudes(point: &rsb_event::Point, algorithm: &Algorithm, to_kev: bool) -> BTreeMap<u64, BTreeMap<usize, f32>> {

    let mut amplitudes = BTreeMap::new();

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = amplitudes.entry(frame.time).or_insert(BTreeMap::new());

                let waveform = process_waveform(&frame_to_waveform(frame));

                for (_, amp) in waveform_to_events(&waveform, algorithm) {
                    let amp = if to_kev {
                        convert_to_kev(&amp, channel.id as u8, algorithm)
                    } else {
                        amp
                    };
                    entry.insert(channel.id as usize, amp);
                }
            }
        }
    }

    amplitudes
}

pub fn amplitudes_to_histogram(
        mut amplitudes: BTreeMap<u64, BTreeMap<usize, f32>>, 
        post_processing: PostProcessingParams,
        histogram: HistogramParams
    ) -> PointHistogram {
    
    let mut last_time: u64 = 0;
    let filtered = amplitudes.iter_mut().filter_map(|(time, channels)| {

        if post_processing.use_dead_time && last_time.abs_diff(*time) < post_processing.effective_dead_time {
            return None;
        }

        last_time = *time;

        if post_processing.merge_close_events {
            for ch_1 in 0..7 {
                for ch_2 in 0..7 {
                    if post_processing.merge_map[ch_1][ch_2]
                        && channels.contains_key(&ch_1)
                        && channels.contains_key(&ch_2)
                    {
                        let amp2 = channels.get(&ch_2).unwrap().to_owned();
                        channels.entry(ch_1).and_modify(|amp| *amp += amp2);
                        channels.remove_entry(&ch_2).unwrap();
                    }
                }
            }
        }

        Some((time, channels))
    });

    let mut histogram = PointHistogram::from(histogram);
    filtered.for_each(|(_, channels)| {
        for (ch_num, amplitude) in channels {
            histogram.add(*ch_num as u8, *amplitude)
        }
    });
    histogram
}

#[derive(Debug, Clone)]
pub struct RawWaveform(pub Vec<i16>);

pub fn frame_to_waveform(frame: &rsb_event::point::channel::block::Frame) -> RawWaveform {
    let waveform_len = frame.data.len() / 2;
    RawWaveform((0..waveform_len)
    .map(|idx| i16::from_le_bytes(frame.data[idx * 2..idx * 2 + 2].try_into().unwrap()))
    .collect::<Vec<_>>())
}

pub fn process_waveform(waveform: &RawWaveform) -> ProcessedWaveform {
    let baseline = waveform.0.iter().take(16).sum::<i16>() as f32 / 16.0;
    ProcessedWaveform(waveform.0.iter().map(|bin| *bin as f32 - baseline).collect::<Vec<_>>())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedWaveform(pub Vec<f32>);

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

pub fn waveform_to_events(waveform: &ProcessedWaveform, algorithm: &Algorithm) -> Vec<(u64, f32)> {
    let (x, y) = waveform.0
        .iter()
        .enumerate()
        .max_by(|first, second| {
            first.1.partial_cmp(second.1).unwrap()
        })
        .unwrap();

    match algorithm {
        Algorithm::Max => vec![(x as u64 * 8, *y)],
        Algorithm::Likhovid { left, right } => {
            let amplitude = {
                let left = if x >= *left { x - left } else { 0 };
                let right = std::cmp::min(waveform.0.len(), x + right);
                let crop = &waveform.0[left..right];
                crop.iter().sum::<f32>() / crop.len() as f32
            };

            vec![(x as u64 * 8, amplitude)]
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
                vec![(pos as u64 * 8, amplitude / 50.0)]
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

pub fn point_to_chunks(point: rsb_event::Point, limit_ns: u64) -> Vec<Vec<(u8, Vec<[f64; 2]>)>> {

    let mut chunks = vec![];
    chunks.push(vec![]);

    for channel in point.channels {
        for block in channel.blocks {
            for frame in block.frames {
                let chunk_num = (frame.time / limit_ns) as usize;
                
                while chunks.len() < chunk_num + 1 {
                    chunks.push(vec![])
                }

                let waveform = process_waveform(&frame_to_waveform(&frame));

                chunks[chunk_num].push((
                    channel.id as u8,
                    waveform.0.iter().enumerate().map(|(idx, y)| {
                        [(frame.time + 8u64 * (idx as u64) - (chunk_num as u64 * limit_ns)) as f64 / 1000.0, *y as f64]
                    }).collect::<Vec<_>>(),
                ));
            }
        }
    }

    chunks
}
