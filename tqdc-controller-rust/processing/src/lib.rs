#[cfg(not(target_arch = "wasm32"))]
use {
    std::collections::BTreeMap,
    numass::protos::rsb_event,
    histogram::PointHistogram
};
use serde::{Serialize, Deserialize};
pub mod histogram;
pub mod utils;

#[derive(PartialEq, Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ProcessingParams {
    pub algorithm: Algorithm,
    pub convert_to_kev: bool,
    pub merge_close_events: bool, // TODO: consider do in more idiomatic way
    pub merge_map: [[bool; 7]; 7],
    pub use_dead_time: bool,
    pub effective_dead_time: u64,
    // TODO: add to KeV corrections
    // TODO: refactor to separate struct and merge with PointHistogram
    pub hist_min: f32,
    pub hist_max: f32,
    pub hist_bins: usize
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Algorithm {
    Max,
    Likhovid {
        left: usize,
        right: usize
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
    [0.21068372, 0.07455444],
    [0.22343008, 0.06619072],
    [0.23688205, 0.0010967255],
    [0.24058165, 0.017612457],
    [0.2430875, 0.07686138],
    [0.23658493, 0.055846214],
    [0.21352, 0.039334297]
];

#[cfg(not(target_arch = "wasm32"))]
pub fn point_to_histogramm(point: &rsb_event::Point, params: ProcessingParams) -> PointHistogram {

    let range = (params.hist_min, params.hist_max);
    let bins = params.hist_bins;

    let mut histogram = PointHistogram::new(range, bins);

    let mut frames = BTreeMap::new();

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = frames.entry(frame.time).or_insert(BTreeMap::new());

                let waveform = frame_to_waveform(frame);
                let amp = waveform_to_event(&waveform, &params.algorithm).1;

                let amp = if params.convert_to_kev {
                    convert_to_kev(&amp, channel.id as u8, &params.algorithm)
                } else { amp };

                entry.insert(channel.id as usize, amp);
            }
        }
    }

    let mut last_time: u64 = 0;
    for (time, mut channels) in frames {

        if params.use_dead_time && last_time.abs_diff(time) < params.effective_dead_time {
            continue;
        }
        last_time = time;

        if params.merge_close_events {
            for ch_1 in 0..7 {
                for ch_2 in 0..7 {
                    if params.merge_map[ch_1][ch_2] && channels.contains_key(&ch_1) && channels.contains_key(&ch_2) {
                        let amp2  = channels.get(&ch_2).unwrap().to_owned();
                        channels.entry(ch_1).and_modify(|amp| *amp += amp2);
                        channels.remove_entry(&ch_2).unwrap();
                    }
                }
            }
        }

        for (ch_num, amplitude) in channels {
            histogram.add(ch_num as u8, amplitude)
        }
    }

    histogram
}

#[cfg(not(target_arch = "wasm32"))]
pub fn frame_to_waveform(frame: &rsb_event::point::channel::block::Frame) -> Vec<i16>{
    let waveform_len = frame.data.len() / 2;
    (0..waveform_len).map(|idx| {
        i16::from_le_bytes(frame.data[idx*2..idx*2+2].try_into().unwrap())
    }).collect::<Vec<_>>()
}

pub fn convert_to_kev(amplitude: &f32, ch_id: u8, algorithm: &Algorithm) -> f32 {
    match algorithm {
        Algorithm::Max => {
            let [a, b] = KEV_COEFF_MAX[ch_id as usize];
            a * *amplitude + b
        },
        Algorithm::Likhovid { .. } => {
            let [a, b] = KEV_COEFF_LIKHOVID[ch_id as usize];
            a * *amplitude + b
        }
    }
}

pub fn waveform_to_event(waveform: &Vec<i16>, algorithm: &Algorithm) -> (u64, f32) {
    let baseline = waveform.iter().take(16).sum::<i16>() as f32 / 16.0;
    let (x, y)  = waveform.iter().enumerate().max_by_key(|(_, amp)| *amp).unwrap();

    match algorithm {
        
        Algorithm::Max => (x as u64 * 8,  *y as f32 - baseline),
        Algorithm::Likhovid { left, right } => {
            // TODO: move to processing
            let amplitude = {
                let left = if x >= *left {x - left} else { 0 };
                let right = std::cmp::min(waveform.len(), x + right);
                let crop =  &waveform[left..right];
                crop.iter().sum::<i16>() as f32 / crop.len() as f32
            };

            (x as u64 * 8, amplitude - baseline)
        }
    }
}

/// Parabolic event amplitude correction correction
pub fn correct_amp(y0: f32, y1: f32, y2: f32) -> (f32, f32) {
    (
        // calculated with SymPy
        (y0 - y2)/(2.0*(y0 - 2.0*y1 + y2)),
        (-(y0*y0)/8.0 + y0*y1 + y0*y2/4.0 - 2.0 * y1 * y1 + y1*y2 - (y2*y2)/8.0)/(y0 - 2.0 * y1 + y2)
    )
}

pub fn find_first_peak(waveform: &Vec<i16>, threshold: f32, baseline: f32) -> usize {
    waveform.iter().enumerate().find(|(idx, amp)| {
        let amp = **amp as f32 - baseline;
        amp > threshold &&
        (*idx == 0 || waveform[idx - 1] as f32 - baseline <= amp)&&
        (*idx == waveform.len() - 1 || waveform[idx + 1] as f32 - baseline <= amp)
    }).map(|(idx, _)| idx).unwrap()
}