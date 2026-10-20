//! Types used in the Numass processing.
//! This module also contains some converters between types.
use std::collections::BTreeMap;

use numass::protos::rsb_event;
use serde::{Deserialize, Serialize};

/// Время (длительность) с проверкой единиц на уровне типов ([`uom`]).
///
/// Все длительности в API крейта имеют этот тип: нельзя сложить наносекунды
/// с миллисекундами или забыть коэффициент — перевод в конкретные единицы
/// происходит только через `Time::new::<...>()` / `.get::<...>()`.
///
/// Сериализуется (serde) числом в базовой единице uom — **секундах**.
pub use uom::si::f64::Time;
/// Единицы времени для `Time::new::<...>()` / `.get::<...>()`.
pub use uom::si::time::{microsecond, millisecond, nanosecond, second};

/// Длительность одного отсчёта АЦП (бина вейвформы) в наносекундах.
///
/// АЦП работает на 125 МГц: 1e9 / 125 = 8 нс.
/// Все времена на осциллограммах — в наносекундах: `бин * TICK_NS`
/// (перевод — через [`bin_time`]).
pub const TICK_NS: u16 = 8;

/// Время в наносекундах: единицы оси x осциллограмм и таймстампов кадров.
///
/// Обратное преобразование для отрисовки — `.get::<nanosecond>()`.
pub fn nanoseconds(value: f64) -> Time {
    Time::new::<nanosecond>(value)
}

/// Время от начала кадра до бина с индексом `bin`: `bin * TICK_NS`.
///
/// Единственная точка перевода «бины АЦП → время».
pub fn bin_time(bin: usize) -> Time {
    nanoseconds(bin as f64 * TICK_NS as f64)
}

/// Целое число наносекунд из [`Time`] — с округлением.
///
/// Базовая единица [`uom`] для времени — секунда, поэтому точный round-trip
/// через f64 требует округления: без него можно получить `…9999999` вместо
/// круглого числа при переводе в целые нс.
pub fn time_ns(time: Time) -> u64 {
    time.get::<nanosecond>().round() as u64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedWaveform(pub Vec<f32>);

pub type NumassFrame = BTreeMap<u8, Vec<i16>>;
/// Numass point conveted to frames.
pub type NumassWaveforms = BTreeMap<u64, NumassFrame>;

pub type NumassFrameFast<'a> = BTreeMap<u8, &'a [i16]>;
/// Numass point conveted to frames (no copy, Point must be alive for this data to be valid).
pub type NumassWaveformsFast<'a> = BTreeMap<u64, NumassFrameFast<'a>>;

/// Numass processed events type (both for processing + postprocessing and processing only).
pub type NumassEvents = BTreeMap<u64, Vec<NumassEvent>>;
/// Numass event (position in waveform in ns, amplitude).
pub type NumassEvent = (u16, FrameEvent);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FrameEvent {
    Event {
        channel: u8,
        amplitude: f32,
        size: u16,
    },
    Overflow {
        channel: u8,
        size: u16,
    },
    Reset {
        size: u16,
    },
    Frame {
        size: u16,
    },
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
    /// Точки для графика: x — время от начала кадра плюс `offset`.
    pub fn to_egui_line(&self, offset: Time) -> Vec<[f64; 2]> {
        self.0
            .iter()
            .enumerate()
            .map(|(x, y)| [(bin_time(x) + offset).get::<nanosecond>(), *y as f64])
            .collect::<Vec<_>>()
    }
}

impl From<RawWaveform> for Vec<[f64; 2]> {
    fn from(waveform: RawWaveform) -> Self {
        waveform
            .0
            .iter()
            .enumerate()
            .map(|(x, y)| [bin_time(x).get::<nanosecond>(), *y as f64])
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
        RawWaveform(
            (0..waveform_len)
                .map(|idx| i16::from_le_bytes(frame.data[idx * 2..idx * 2 + 2].try_into().unwrap()))
                .collect::<Vec<_>>(),
        )
    }
}

impl From<&rsb_event::point::channel::block::Frame> for RawWaveform {
    fn from(frame: &rsb_event::point::channel::block::Frame) -> Self {
        let waveform_len = frame.data.len() / 2;
        RawWaveform(
            (0..waveform_len)
                .map(|idx| i16::from_le_bytes(frame.data[idx * 2..idx * 2 + 2].try_into().unwrap()))
                .collect::<Vec<_>>(),
        )
    }
}

impl From<ProcessedWaveform> for Vec<[f64; 2]> {
    fn from(waveform: ProcessedWaveform) -> Self {
        waveform
            .0
            .iter()
            .enumerate()
            .map(|(x, y)| [bin_time(x).get::<nanosecond>(), *y as f64])
            .collect::<Vec<_>>()
    }
}
