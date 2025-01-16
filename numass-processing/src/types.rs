//! Types used in the Numass processing.
//! This module also contains some converters between types.
use std::collections::BTreeMap;

use numass::protos::rsb_event;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedWaveform(pub Vec<f32>);

pub type NumassFrame<'a> = BTreeMap<u8, &'a [i16]>;
/// Numass point conveted to frames.
pub type NumassWaveforms<'a> = BTreeMap<u64, NumassFrame<'a>>;
/// Numass processed events type (both for processing + postprocessing and processing only).
pub type NumassEvents = BTreeMap<u64, Vec<NumassEvent>>;
/// Numass event (position in waveform, amplitude).
pub type NumassEvent = (u16, FrameEvent);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FrameEvent {
    Event { channel: u8, amplitude: f32, size: u16 },
    Overflow { channel: u8, size: u16 },
    Reset { size: u16 },
    Frame { size: u16 },
}

impl std::hash::Hash for FrameEvent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
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

impl From<&[i16]> for ProcessedWaveform {
    fn from(data: &[i16]) -> Self {
        Self(data.iter().map(|bin| *bin as f32).collect::<Vec<_>>())
    }
}

impl From<&RawWaveform> for ProcessedWaveform {
    fn from(data: &RawWaveform) -> Self {
        Self(data.0.iter().map(|bin| *bin as f32).collect::<Vec<_>>())
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

impl From<ProcessedWaveform> for Vec<[f64; 2]> {
    fn from(waveform: ProcessedWaveform) -> Self {
        waveform.0.iter()
            .enumerate()
            .map(|(x, y)| [x as f64, *y as f64])
            .collect::<Vec<_>>()
    }
}
