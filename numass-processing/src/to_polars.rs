//! # Polars
//! Конверсия табличных данных numass в [`DataFrame`] библиотеки
//! [polars](https://docs.rs/polars) за фичей `polars`.
//!
//! [`ToPolars`] реализован для:
//! - сырых кадров: [`NumassFrame`], [`NumassWaveforms`] и их `Fast`-вариантов,
//!   [`RawWaveform`];
//! - обработанных отсчётов: [`ChannelSamples`], [`ProcessedFrame`],
//!   [`ProcessedPoint`], [`ProcessedWaveform`] и [`FrameIn`] (сырой и
//!   обработанный кадр, выровненные по бинам);
//! - событий (амплитуд): [`FrameEvent`], [`NumassEvent`], [`NumassEvents`].
//!
//! Конфигурации и параметры обработки ([`PreprocessParams`](crate::preprocess::PreprocessParams),
//! [`Algorithm`](crate::process::Algorithm) и т.п.), метаданные точки
//! ([`StaticPointParams`](crate::preprocess::StaticPointParams)) и гистограммы
//! ([`PointHistogram`](crate::histogram::PointHistogram)) намеренно не
//! конвертируются: гистограммы нужны графике Rust. Сырая точка протокола
//! (`rsb_event::Point`) тоже: сначала её нужно разобрать
//! [`extract_waveforms`](crate::preprocess::extract_waveforms).
//!
//! ## Конвенции
//! - Каналы — идентификаторы из данных (0-based); в отображении crate они
//!   сдвинуты на единицу (`ch# N+1`).
//! - Все времена — наносекунды: `time` — время кадра от начала набора точки,
//!   `position` — позиция события от начала кадра.
//! - Сырые отсчёты — коды АЦП (i16 из данных), в таблицах хранятся как i64;
//!   обработанные отсчёты — f32.
//! - `size` событий измеряется в отсчётах по 8 нс.
//! - Таблицы событий и отдельных осциллограмм — «длинные»: одна строка на
//!   отсчёт или событие. Сырые кадры ([`NumassFrame`], [`NumassWaveforms`] и
//!   `Fast`) и [`ProcessedPoint`] — одна строка на осциллограмму канала,
//!   `value` — вектор значений бинов. Пустые данные дают таблицу высоты 0.

use std::collections::BTreeSet;

use polars::prelude::*;

use crate::{
    preprocess::{ChannelSamples, ProcessedFrame, ProcessedPoint},
    process::FrameIn,
    types::{
        FrameEvent, NumassEvent, NumassEvents, NumassFrame, NumassFrameFast, NumassWaveforms,
        NumassWaveformsFast, ProcessedWaveform, RawWaveform,
    },
};

/// Конверсия в polars-таблицу [`DataFrame`].
///
/// Структуру конкретных таблиц описывают реализации (документация у каждого
/// `impl`); общие конвенции — в [документации модуля](crate::to_polars).
pub trait ToPolars {
    /// Таблица polars с данными этого объекта.
    fn to_polars(&self) -> DataFrame;
}

/// Собирает таблицу из колонок. Все колонки должны быть одной длины.
fn frame(columns: Vec<Column>) -> DataFrame {
    let height = columns.first().map_or(0, Column::len);
    DataFrame::new(height, columns).expect("to_polars: columns of equal length")
}

// ===================== Обработанные отсчёты =====================

impl ToPolars for ChannelSamples {
    /// Одна строка на отсчёт: `index` (позиция в выборке), `delay`, `value`.
    /// Позиция соответствующего бина сырого кадра — `index + delay`.
    fn to_polars(&self) -> DataFrame {
        let height = self.samples.len();
        frame(vec![
            Column::new("index".into(), (0..height as i64).collect::<Vec<_>>()),
            Column::new("delay".into(), vec![self.delay as i64; height]),
            Column::new("value".into(), self.samples.clone()),
        ])
    }
}

impl ToPolars for ProcessedFrame {
    /// Одна строка на отсчёт: `channel`, `index`, `delay`, `value`.
    fn to_polars(&self) -> DataFrame {
        let mut channel = Vec::new();
        let mut index = Vec::new();
        let mut delay = Vec::new();
        let mut value = Vec::new();
        for (ch, samples) in self {
            for (idx, sample) in samples.samples.iter().enumerate() {
                channel.push(*ch as i64);
                index.push(idx as i64);
                delay.push(samples.delay as i64);
                value.push(*sample);
            }
        }
        frame(vec![
            Column::new("channel".into(), channel),
            Column::new("index".into(), index),
            Column::new("delay".into(), delay),
            Column::new("value".into(), value),
        ])
    }
}

impl ToPolars for ProcessedPoint {
    /// Одна строка на осциллограмму канала: `time` (нс от начала набора точки),
    /// `channel`, `delay`, `value` (вектор значений бинов).
    /// Позиция бина `value[i]` в сыром кадре — `i + delay`.
    fn to_polars(&self) -> DataFrame {
        let rows: usize = self.values().map(|frame| frame.len()).sum();
        let bins: usize = self
            .values()
            .map(|frame| frame.values().map(|ch| ch.samples.len()).sum::<usize>())
            .sum();
        let mut time = Vec::with_capacity(rows);
        let mut channel = Vec::with_capacity(rows);
        let mut delay = Vec::with_capacity(rows);
        let mut value = ListPrimitiveChunkedBuilder::<Float32Type>::new(
            "value".into(),
            rows,
            bins,
            DataType::Float32,
        );
        for (frame_time, frame) in self {
            for (ch, samples) in frame {
                time.push(*frame_time as i64);
                channel.push(*ch as i64);
                delay.push(samples.delay as i64);
                value.append_slice(&samples.samples);
            }
        }
        frame(vec![
            Column::new("time".into(), time),
            Column::new("channel".into(), channel),
            Column::new("delay".into(), delay),
            value.finish().into_column(),
        ])
    }
}

// ===================== Сырые кадры =====================

fn adc_list() -> ListPrimitiveChunkedBuilder<Int64Type> {
    ListPrimitiveChunkedBuilder::<Int64Type>::new("value".into(), 0, 0, DataType::Int64)
}

fn push_waveform(
    channel: &mut Vec<i64>,
    value: &mut ListPrimitiveChunkedBuilder<Int64Type>,
    ch: u8,
    waveform: &[i16],
) {
    channel.push(ch as i64);
    value.append_values_iter(waveform.iter().map(|sample| i64::from(*sample)));
}

/// Таблица одного кадра: `channel`, `value` (вектор кодов АЦП).
fn raw_frame_table<'s>(channels: impl Iterator<Item = (u8, &'s [i16])>) -> DataFrame {
    let mut channel = Vec::new();
    let mut value = adc_list();
    for (ch, waveform) in channels {
        push_waveform(&mut channel, &mut value, ch, waveform);
    }
    frame(vec![
        Column::new("channel".into(), channel),
        value.finish().into_column(),
    ])
}

/// Таблица точки: `time`, `channel`, `value` (вектор кодов АЦП).
fn raw_point_table<'a, C, I>(frames: I) -> DataFrame
where
    C: Iterator<Item = (u8, &'a [i16])>,
    I: Iterator<Item = (u64, C)>,
{
    let mut time = Vec::new();
    let mut channel = Vec::new();
    let mut value = adc_list();
    for (frame_time, channels) in frames {
        for (ch, waveform) in channels {
            time.push(frame_time as i64);
            push_waveform(&mut channel, &mut value, ch, waveform);
        }
    }
    frame(vec![
        Column::new("time".into(), time),
        Column::new("channel".into(), channel),
        value.finish().into_column(),
    ])
}

impl ToPolars for NumassFrame {
    /// Одна строка на канал: `channel`, `value` (вектор кодов АЦП).
    fn to_polars(&self) -> DataFrame {
        raw_frame_table(self.iter().map(|(ch, waveform)| (*ch, waveform.as_slice())))
    }
}

impl ToPolars for NumassFrameFast<'_> {
    /// Одна строка на канал: `channel`, `value` (вектор кодов АЦП).
    fn to_polars(&self) -> DataFrame {
        raw_frame_table(self.iter().map(|(ch, waveform)| (*ch, *waveform)))
    }
}

impl ToPolars for NumassWaveforms {
    /// Одна строка на осциллограмму канала: `time` (нс), `channel`, `value`.
    fn to_polars(&self) -> DataFrame {
        raw_point_table(self.iter().map(|(time, frame)| {
            (
                *time,
                frame
                    .iter()
                    .map(|(ch, waveform)| (*ch, waveform.as_slice())),
            )
        }))
    }
}

impl ToPolars for NumassWaveformsFast<'_> {
    /// Одна строка на осциллограмму канала: `time` (нс), `channel`, `value`.
    fn to_polars(&self) -> DataFrame {
        raw_point_table(
            self.iter()
                .map(|(time, frame)| (*time, frame.iter().map(|(ch, waveform)| (*ch, *waveform)))),
        )
    }
}

impl ToPolars for RawWaveform {
    /// Одна строка на отсчёт: `index`, `value` (код АЦП).
    fn to_polars(&self) -> DataFrame {
        let height = self.0.len();
        frame(vec![
            Column::new("index".into(), (0..height as i64).collect::<Vec<_>>()),
            Column::new(
                "value".into(),
                self.0
                    .iter()
                    .map(|sample| i64::from(*sample))
                    .collect::<Vec<_>>(),
            ),
        ])
    }
}

impl ToPolars for ProcessedWaveform {
    /// Одна строка на отсчёт: `index`, `value`.
    fn to_polars(&self) -> DataFrame {
        let height = self.0.len();
        frame(vec![
            Column::new("index".into(), (0..height as i64).collect::<Vec<_>>()),
            Column::new("value".into(), self.0.clone()),
        ])
    }
}

// ===================== События =====================

/// (kind, channel, amplitude, size) события.
fn event_parts(event: &FrameEvent) -> (&'static str, Option<i64>, Option<f32>, i64) {
    match event {
        FrameEvent::Event {
            channel,
            amplitude,
            size,
        } => (
            "event",
            Some(*channel as i64),
            Some(*amplitude),
            *size as i64,
        ),
        FrameEvent::Overflow { channel, size } => {
            ("overflow", Some(*channel as i64), None, *size as i64)
        }
        FrameEvent::Reset { size } => ("reset", None, None, *size as i64),
        FrameEvent::Frame { size } => ("frame", None, None, *size as i64),
    }
}

impl ToPolars for FrameEvent {
    /// Одна строка: `kind` (`event`/`overflow`/`reset`/`frame`), `channel`,
    /// `amplitude` (кэВ, если включена конверсия, иначе коды АЦП), `size`
    /// (в отсчётах по 8 нс).
    fn to_polars(&self) -> DataFrame {
        let (kind, channel, amplitude, size) = event_parts(self);
        frame(vec![
            Column::new("kind".into(), vec![kind]),
            Column::new("channel".into(), vec![channel]),
            Column::new("amplitude".into(), vec![amplitude]),
            Column::new("size".into(), vec![size]),
        ])
    }
}

impl ToPolars for NumassEvent {
    /// Одна строка: `position` (нс от начала кадра) + колонки [`FrameEvent`].
    fn to_polars(&self) -> DataFrame {
        let (kind, channel, amplitude, size) = event_parts(&self.1);
        frame(vec![
            Column::new("position".into(), vec![self.0 as i64]),
            Column::new("kind".into(), vec![kind]),
            Column::new("channel".into(), vec![channel]),
            Column::new("amplitude".into(), vec![amplitude]),
            Column::new("size".into(), vec![size]),
        ])
    }
}

impl ToPolars for Vec<NumassEvent> {
    /// События одного кадра — колонки как у [`NumassEvent`], по строке на событие.
    fn to_polars(&self) -> DataFrame {
        let mut position = Vec::with_capacity(self.len());
        let mut kind = Vec::with_capacity(self.len());
        let mut channel = Vec::with_capacity(self.len());
        let mut amplitude = Vec::with_capacity(self.len());
        let mut size = Vec::with_capacity(self.len());
        for (pos, event) in self {
            let (k, ch, amp, len) = event_parts(event);
            position.push(*pos as i64);
            kind.push(k);
            channel.push(ch);
            amplitude.push(amp);
            size.push(len);
        }
        frame(vec![
            Column::new("position".into(), position),
            Column::new("kind".into(), kind),
            Column::new("channel".into(), channel),
            Column::new("amplitude".into(), amplitude),
            Column::new("size".into(), size),
        ])
    }
}

impl ToPolars for NumassEvents {
    /// Одна строка на событие: `time` (нс от начала набора точки), `position`
    /// (нс от начала кадра), `kind`, `channel`, `amplitude`, `size`.
    fn to_polars(&self) -> DataFrame {
        let total: usize = self.values().map(Vec::len).sum();
        let mut time = Vec::with_capacity(total);
        let mut position = Vec::with_capacity(total);
        let mut kind = Vec::with_capacity(total);
        let mut channel = Vec::with_capacity(total);
        let mut amplitude = Vec::with_capacity(total);
        let mut size = Vec::with_capacity(total);
        for (frame_time, events) in self {
            for (pos, event) in events {
                let (k, ch, amp, len) = event_parts(event);
                time.push(*frame_time as i64);
                position.push(*pos as i64);
                kind.push(k);
                channel.push(ch);
                amplitude.push(amp);
                size.push(len);
            }
        }
        frame(vec![
            Column::new("time".into(), time),
            Column::new("position".into(), position),
            Column::new("kind".into(), kind),
            Column::new("channel".into(), channel),
            Column::new("amplitude".into(), amplitude),
            Column::new("size".into(), size),
        ])
    }
}

// ===================== Кадр: сырой + обработанный =====================

impl ToPolars for FrameIn<'_> {
    /// Кадр, выровненный по сырым бинам: `channel`, `bin`, `raw` (код АЦП),
    /// `processed`. Колонка `processed` — null в первых `delay` бинах (сдвиг
    /// FIR) и для каналов, отсутствующих в одной из сторон.
    fn to_polars(&self) -> DataFrame {
        let channels: BTreeSet<u8> = self
            .processed
            .keys()
            .chain(self.raw.keys())
            .copied()
            .collect();
        let mut channel = Vec::new();
        let mut bin = Vec::new();
        let mut raw = Vec::new();
        let mut processed = Vec::new();
        for ch in channels {
            let samples = self.processed.get(&ch);
            let waveform = self.raw.get(&ch);
            let height = samples
                .map(|s| s.delay + s.samples.len())
                .unwrap_or(0)
                .max(waveform.map_or(0, |w| w.len()));
            for idx in 0..height {
                channel.push(ch as i64);
                bin.push(idx as i64);
                raw.push(
                    waveform
                        .and_then(|w| w.get(idx))
                        .map(|sample| *sample as i64),
                );
                processed.push(match samples {
                    Some(s) if idx >= s.delay && idx - s.delay < s.samples.len() => {
                        Some(s.samples[idx - s.delay])
                    }
                    _ => None,
                });
            }
        }
        frame(vec![
            Column::new("channel".into(), channel),
            Column::new("bin".into(), bin),
            Column::new("raw".into(), raw),
            Column::new("processed".into(), processed),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::preprocess::{PreprocessParams, apply_waveforms};

    fn ints(df: &DataFrame, name: &str) -> Vec<Option<i64>> {
        let column = df.column(name).expect("column exists");
        let values = column.i64().expect("i64 column");
        (0..values.len()).map(|idx| values.get(idx)).collect()
    }

    fn list_i64(df: &DataFrame, name: &str, row: usize) -> Vec<i64> {
        let series = df
            .column(name)
            .expect("column exists")
            .list()
            .expect("list column")
            .get_as_series(row)
            .expect("list row");
        let values = series.i64().expect("i64 values");
        (0..values.len())
            .map(|idx| values.get(idx).expect("value"))
            .collect()
    }

    fn list_f32(df: &DataFrame, name: &str, row: usize) -> Vec<f32> {
        let series = df
            .column(name)
            .expect("column exists")
            .list()
            .expect("list column")
            .get_as_series(row)
            .expect("list row");
        let values = series.f32().expect("f32 values");
        (0..values.len())
            .map(|idx| values.get(idx).expect("value"))
            .collect()
    }

    fn floats(df: &DataFrame, name: &str) -> Vec<Option<f32>> {
        let column = df.column(name).expect("column exists");
        let values = column.f32().expect("f32 column");
        (0..values.len()).map(|idx| values.get(idx)).collect()
    }

    fn strings(df: &DataFrame, name: &str) -> Vec<Option<String>> {
        let column = df.column(name).expect("column exists");
        let values = column.str().expect("str column");
        (0..values.len())
            .map(|idx| values.get(idx).map(str::to_string))
            .collect()
    }

    fn column_names(df: &DataFrame) -> Vec<String> {
        df.get_column_names()
            .iter()
            .map(|name| name.to_string())
            .collect()
    }

    #[test]
    fn raw_frame_is_one_waveform_per_row() {
        let frame: NumassFrame = BTreeMap::from([(1u8, vec![10i16, -20]), (0u8, vec![1i16, 2, 3])]);
        let df = frame.to_polars();

        assert_eq!(df.height(), 2);
        assert_eq!(column_names(&df), ["channel", "value"]);
        assert_eq!(ints(&df, "channel"), [Some(0), Some(1)]);
        assert_eq!(list_i64(&df, "value", 0), [1, 2, 3]);
        assert_eq!(list_i64(&df, "value", 1), [10, -20]);
    }

    #[test]
    fn fast_waveforms_are_covered() {
        let waveform = [5i16, 6];
        let frame = BTreeMap::from([(0u8, waveform.as_slice())]);
        let waveforms: NumassWaveformsFast = BTreeMap::from([(7u64, frame)]);
        let df = waveforms.to_polars();

        assert_eq!(df.height(), 1);
        assert_eq!(column_names(&df), ["time", "channel", "value"]);
        assert_eq!(ints(&df, "time"), [Some(7)]);
        assert_eq!(ints(&df, "channel"), [Some(0)]);
        assert_eq!(list_i64(&df, "value", 0), [5, 6]);
    }

    #[test]
    fn processed_point_table_matches_preprocessing() {
        let mut waveform = vec![100i16; 30];
        waveform.extend([80i16; 10]);
        let frame = BTreeMap::from([(0u8, waveform.as_slice())]);
        let waveforms = BTreeMap::from([(5u64, frame)]);
        let processed = apply_waveforms(&waveforms, &PreprocessParams::average().steps);
        let df = processed.to_polars();

        assert_eq!(df.height(), 1);
        assert_eq!(column_names(&df), ["time", "channel", "delay", "value"]);
        assert_eq!(
            df.column("value").expect("value").dtype(),
            &DataType::List(Box::new(DataType::Float32))
        );
        assert_eq!(ints(&df, "time"), [Some(5)]);
        assert_eq!(ints(&df, "channel"), [Some(0)]);
        assert_eq!(ints(&df, "delay"), [Some(0)]);

        let values = list_f32(&df, "value", 0);
        assert_eq!(values.len(), 40);
        assert!(values[..30].iter().all(|value| value.abs() < 1e-4));
        assert!(values[30..].iter().all(|value| (value - 20.0).abs() < 1e-4));
    }

    #[test]
    fn events_table_has_kinds_and_nulls() {
        let events: NumassEvents = BTreeMap::from([(
            100u64,
            vec![
                (
                    16u16,
                    FrameEvent::Event {
                        channel: 3,
                        amplitude: 55.5,
                        size: 10,
                    },
                ),
                (
                    32u16,
                    FrameEvent::Overflow {
                        channel: 1,
                        size: 4,
                    },
                ),
                (48u16, FrameEvent::Reset { size: 8 }),
            ],
        )]);
        let df = events.to_polars();

        assert_eq!(df.height(), 3);
        assert_eq!(ints(&df, "time"), [Some(100); 3]);
        assert_eq!(ints(&df, "position"), [Some(16), Some(32), Some(48)]);
        assert_eq!(
            strings(&df, "kind"),
            [
                Some("event".to_string()),
                Some("overflow".to_string()),
                Some("reset".to_string())
            ]
        );
        assert_eq!(ints(&df, "channel"), [Some(3), Some(1), None]);
        assert_eq!(floats(&df, "amplitude"), [Some(55.5), None, None]);
        assert_eq!(ints(&df, "size"), [Some(10), Some(4), Some(8)]);
    }

    #[test]
    fn empty_event_vec_keeps_schema() {
        let events: Vec<NumassEvent> = vec![
            (0u16, FrameEvent::Frame { size: 5 }),
            (8u16, FrameEvent::Frame { size: 6 }),
        ];
        let df = events.to_polars();
        assert_eq!(df.height(), 2);
        assert_eq!(ints(&df, "position"), [Some(0), Some(8)]);
        assert_eq!(ints(&df, "channel"), [None, None]);

        let empty: Vec<NumassEvent> = Vec::new();
        let df = empty.to_polars();
        assert_eq!(df.height(), 0);
        assert_eq!(
            column_names(&df),
            ["position", "kind", "channel", "amplitude", "size"]
        );
    }

    #[test]
    fn frame_in_aligns_processed_to_raw_bins() {
        let waveform = vec![10i16, 20, 30, 40];
        let raw: NumassFrameFast = BTreeMap::from([(0u8, waveform.as_slice())]);
        let processed: ProcessedFrame = BTreeMap::from([(
            0u8,
            ChannelSamples {
                samples: vec![1.0, 2.0],
                delay: 2,
            },
        )]);
        let input = FrameIn {
            processed: &processed,
            raw: &raw,
        };
        let df = input.to_polars();

        assert_eq!(df.height(), 4);
        assert_eq!(column_names(&df), ["channel", "bin", "raw", "processed"]);
        assert_eq!(ints(&df, "bin"), [Some(0), Some(1), Some(2), Some(3)]);
        assert_eq!(ints(&df, "raw"), [Some(10), Some(20), Some(30), Some(40)]);
        assert_eq!(floats(&df, "processed"), [None, None, Some(1.0), Some(2.0)]);
    }
}
