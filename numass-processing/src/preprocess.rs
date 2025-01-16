//! # Processing
//! This module contains preprocessing calclulations per point.
//!

use std::collections::BTreeSet;

use numass::{protos::rsb_event, ExternalMeta, NumassMeta, Reply};
use serde::{Deserialize, Serialize};

use crate::{histogram::PointHistogram, process::{extract_waveforms, Algorithm}, utils::correct_frame_time};

/// Размер блока, который будет вырезан, если в нем обнаружены проблемы (в нс)
pub const CUTOFF_BIN_SIZE: u64 = 1_000_000_000;

/// Размер блока, который будет проверяться на наличие проблем (в нс)
const CHECK_BIN_SIZE: u64 = 10_000_000;

/// Порог по HV для проверки точки (точки с HV выше не будут проверяться)
const CHECK_HV_THRESHOLD: f32 = 16e3;


/// Неизменяемые параметры, необходимые для обработки кадра
/// могут либо задаваться статично, либо на каждую точку
/// TODO: add default derive
#[derive(Clone, Serialize, Deserialize)]
pub struct PreprocessParams {
    pub baseline: Option<Vec<f32>>, // TODO: make more versatile
    
    /// номера блоков, которые нужно исключить из анализа
    /// размер блока равен [CUTOFF_BIN_SIZE](crate::preprocess::CUTOFF_BIN_SIZE)
    pub bad_blocks: BTreeSet<usize>
}

impl PreprocessParams {
    pub fn from_point(meta: Option<NumassMeta>, point: &rsb_event::Point, algo: &Algorithm) -> Self {
        
        let (acquisition_time, hv) =  if let Some(NumassMeta::Reply(Reply::AcquirePoint {
            acquisition_time,
            external_meta: Some(ExternalMeta {
                hv1_value: Some(hv),
                ..
            }),
            ..
        })) = meta {
            (acquisition_time, hv)
        } else {
            panic!("acquisition_time and/or hv1_value not found in metadata")
        };

        let bad_blocks = if hv > CHECK_HV_THRESHOLD {
            BTreeSet::new()
        } else {

            let mut trigger_density_local = PointHistogram::new_step(0.0..(acquisition_time * 1e9), CHECK_BIN_SIZE as f32);

            for channel in &point.channels {
                for block in &channel.blocks {
                    for frame in &block.frames {
                        trigger_density_local.add(0, correct_frame_time(frame.time) as f32);
                    }
                }
            }

            let mut bad_blocks = BTreeSet::new();

            trigger_density_local.channels[&0].iter().enumerate().for_each(|(idx, count)| {
                if *count == 0.0 {
                    let block_idx = (idx as u64 * CHECK_BIN_SIZE) / CUTOFF_BIN_SIZE; 
                    bad_blocks.insert(block_idx as usize);
                }
            });

            bad_blocks
        };
        
        Self {
            baseline: Some(baseline_from_point(point, algo)),
            bad_blocks
        }
    }
}

/// convert point to amplitudes histogram
/// used in [baseline_from_point]
/// extracted into single function for easier testing
fn point_to_amp_hist(point: &rsb_event::Point, algo: &Algorithm) -> PointHistogram {
    let (left, center, right) = match algo {
        Algorithm::Trapezoid {
            left,
            center,
            right,
            ..
        } => (*left, *center, *right),
        _ => panic!("not implemented"),
    };

    let waveforms = extract_waveforms(point);

    let mut amps = PointHistogram::new_step(-5.0..120.0, 0.5);

    for (_, frames) in waveforms {
        for (channel, waveform) in frames {
            // TODO: search for another implementations in code and merge them
            let filtered = waveform
                .windows(left + center + right)
                .map(|window| {
                    (window[left + center..].iter().map(|val| *val as i32).sum::<i32>()
                        - window[..left].iter().map(|val| *val as i32).sum::<i32>()) as f32
                        / (left + right) as f32
                })
                .collect::<Vec<_>>();

            amps.add_batch(channel, filtered);
        }
    }

    amps
}

/// extact baseline for channels from point
/// each channel is converted to amplitude histogramm
/// and then baseline is calculated as histogramm peak
fn baseline_from_point(point: &rsb_event::Point, algo: &Algorithm) -> Vec<f32> {
    let mut baselines = vec![0.0; 7];

    let amps = point_to_amp_hist(point, algo);

    for (ch, hist) in amps.channels {
        let mut max_idx = 0;
        for (idx, amp) in hist.iter().enumerate() {
            if *amp > hist[max_idx] {
                max_idx = idx;
            }
        }

        baselines[ch as usize] = amps.x[max_idx];
    }

    baselines
}
