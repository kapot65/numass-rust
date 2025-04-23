//! # Processing
//! This module contains preprocessing calclulations per point.
//!

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDateTime;
use numass::{
    protos::rsb_event::{self, point::channel::block::Frame},
    ExternalMeta, NumassMeta, Reply,
};
use serde::{Deserialize, Serialize};

use crate::{
    histogram::PointHistogram,
    process::Algorithm,
    types::{NumassWaveforms, NumassWaveformsFast},
    utils::correct_frame_time,
};

/// Размер блока, который будет вырезан, если в нем обнаружены проблемы (в нс)
pub const CUTOFF_BIN_SIZE: u64 = 1_000_000_000;

/// Размер блока, который будет проверяться на наличие проблем (в нс)
pub const CHECK_BIN_SIZE: u64 = 10_000_000;

/// Порог по HV для проверки точки (точки с HV выше не будут проверяться)
pub const CHECK_HV_THRESHOLD: f32 = 16e3;

/// Неизменяемые параметры, необходимые для обработки кадра
/// могут либо задаваться статично, либо на каждую точку
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Preprocess {
    pub baseline: Option<[f32;7]>, // TODO: make more versatile

    /// предполагаемая HV точки
    pub hv: f32,

    /// Время начала набора точки
    pub start_time: NaiveDateTime,

    /// время набора точки в наносекундах
    pub acquisition_time: u64,

    /// длина кадра в наносекундах
    pub frame_len: u64,

    /// номера блоков, которые нужно исключить из анализа
    /// размер блока равен [CUTOFF_BIN_SIZE](crate::preprocess::CUTOFF_BIN_SIZE)
    pub bad_blocks: BTreeSet<usize>,
}

impl Preprocess {

    pub fn from_point(
        meta: Option<NumassMeta>,
        point: &rsb_event::Point,
        algo: &Algorithm,
    ) -> Self {
        let (acquisition_time, hv, start_time) =
            if let Some(NumassMeta::Reply(Reply::AcquirePoint {
                acquisition_time,
                start_time,
                external_meta:
                    Some(ExternalMeta {
                        hv1_value: Some(hv),
                        ..
                    }),
                ..
            })) = meta
            {
                ((acquisition_time * 1e9) as u64, hv, start_time)
            } else {
                panic!("acquisition_time and/or hv1_value not found in metadata")
            };

        let bad_blocks = if hv > CHECK_HV_THRESHOLD {
            BTreeSet::new()
        } else {
            let mut trigger_density_local =
                PointHistogram::new_step(0.0..(acquisition_time as f32), CHECK_BIN_SIZE as f32);

            for channel in &point.channels {
                for block in &channel.blocks {
                    for frame in &block.frames {
                        trigger_density_local.add(0, correct_frame_time(frame.time) as f32);
                    }
                }
            }

            let mut bad_blocks = BTreeSet::new();

            trigger_density_local.channels[&0]
                .iter()
                .enumerate()
                .for_each(|(idx, count)| {
                    if ((idx + 1) as u64 * CHECK_BIN_SIZE) <= acquisition_time && *count == 0.0 {
                        let block_idx = (idx as u64 * CHECK_BIN_SIZE) / CUTOFF_BIN_SIZE;
                        bad_blocks.insert(block_idx as usize);
                    }
                });

            bad_blocks
        };

        let frame_len = ((point
            .channels.first().unwrap()
            .blocks.first().unwrap()
            .frames.first().unwrap()
            .data.len() / 2) * 8) as u64;

        let baseline = match &algo {
            Algorithm::Trapezoid { .. } => {
                Some(baseline_from_point(point, algo))
            }
            Algorithm::Max => None,
            Algorithm::FirstPeak { .. } => None,
            Algorithm::Likhovid { .. } => None,
            Algorithm::LongDiff { .. } => None,
        };

        Self {
            baseline,
            acquisition_time,
            start_time,
            frame_len,
            hv,
            bad_blocks,
        }
    }
    /// calculate effective time of acquisition after removing bad blocks (in nanoseconds)
    /// `acquisition_time - bad_blocks_count as u64 * CUTOFF_BIN_SIZE`
    pub fn effective_time(&self) -> u64 {
        let bad_blocks_count = self.bad_blocks.len();
        self.acquisition_time - bad_blocks_count as u64 * CUTOFF_BIN_SIZE
    }
}

pub fn emulate_fir(waveform: &[i16], right: usize, center: usize, left: usize) -> Vec<f32> {
    waveform
        .windows(left + center + right)
        .map(|window| {
            (window[left + center..]
                .iter()
                .map(|val| *val as i32)
                .sum::<i32>()
                - window[..left].iter().map(|val| *val as i32).sum::<i32>()) as f32
                / (left + right) as f32
        })
        .collect::<Vec<_>>()
}

/// convert point to amplitudes histogram
/// used in [baseline_from_point]
/// extracted into single function for easier testing
pub fn point_to_amp_hist(point: &rsb_event::Point, algo: &Algorithm) -> PointHistogram {
    let (left, center, right) = match algo {
        Algorithm::Trapezoid {
            left,
            center,
            right,
            ..
        } => (*left as usize, *center as usize, *right as usize),
        _ => panic!("not implemented"),
    };

    let waveforms = extract_waveforms(point);

    let mut amps = PointHistogram::new_step(-5.0..120.0, 0.5);

    for (_, frames) in waveforms {
        for (channel, waveform) in frames {
            let filtered = emulate_fir(waveform, right, center, left);
            amps.add_batch(channel, filtered);
        }
    }

    amps
}

pub fn frame_to_waveform(frame: &Frame) -> &[i16] {
    unsafe {
        std::slice::from_raw_parts(
            frame.data.as_ptr() as *const i16,
            frame.data.len() / 2,
        )
    }
}

/// remap waveforms from protobuf message to more convenient format (no copy).
pub fn extract_waveforms(point: &rsb_event::Point) -> NumassWaveformsFast {
    let mut waveforms = BTreeMap::new();

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = waveforms
                    .entry(correct_frame_time(frame.time))
                    .or_insert(BTreeMap::new());
                entry.insert(channel.id as u8, frame_to_waveform(frame));
            }
        }
    }
    waveforms
}

pub fn waveforms_fast_copy(waveforms: NumassWaveformsFast) -> NumassWaveforms {
    waveforms
        .iter()
        .map(|(&time, channels)| {
            (
                time,
                channels
                    .iter()
                    .map(|(&channel, &waveform)| (channel, waveform.to_vec()))
                    .collect::<BTreeMap<_, _>>(),
            )
        })
        .collect::<BTreeMap<_, _>>()
}

/// extact baseline for channels from point
/// each channel is converted to amplitude histogramm
/// and then baseline is calculated as histogramm peak
fn baseline_from_point(point: &rsb_event::Point, algo: &Algorithm) -> [f32; 7] {
    let mut baselines = [0.0; 7];

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
