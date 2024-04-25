//! Types used in the Numass processing.
//! This module also contains some converters between types.
use std::collections::BTreeMap;

use numass::protos::rsb_event;
use serde::{Deserialize, Serialize};

pub type NumassFrame = BTreeMap<u8, ProcessedWaveform>;
/// Numass point conveted to frames.
pub type NumassWaveforms = BTreeMap<u64, NumassFrame>;
/// Numass processed events type (both for processing + postprocessing and processing only).
pub type NumassEvents = BTreeMap<u64, BTreeMap<usize, Vec<NumassEvent>>>;
/// Numass event (position in waveform, amplitude).
pub type NumassEvent = (u16, f32);

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
