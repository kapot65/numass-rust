//! # Processing
//! This module contains preprocessing calculations per point.
//!

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDateTime;
use numass::{
    ExternalMeta, NumassMeta, Reply,
    protos::rsb_event::{self, point::channel::block::Frame},
};
use serde::{Deserialize, Serialize};

use crate::{
    histogram::PointHistogram,
    script::{PreprocessHost, ScriptRuntime},
    types::{
        NumassFrameFast, NumassWaveforms, NumassWaveformsFast, Time, bin_time, nanoseconds, time_ns,
    },
    utils::correct_frame_time,
};

#[cfg(feature = "egui")]
use {
    crate::{
        types::{ProcessedWaveform, nanosecond},
        utils::color_for_index,
    },
    egui_plot::{Line, LineStyle, PlotUi},
};

/// Размер блока, который будет вырезан, если в нем обнаружены проблемы (в нс)
pub const CUTOFF_BIN_SIZE: u64 = 1_000_000_000;

/// Размер блока, который будет проверяться на наличие проблем (в нс)
pub const CHECK_BIN_SIZE: u64 = 10_000_000;

/// Порог по HV для проверки точки (точки с HV выше не будут проверяться)
pub const CHECK_HV_THRESHOLD: f32 = 16e3;

pub const FIR_LEFT: u16 = 6;
pub const FIR_CENTER: u16 = 15;
pub const FIR_RIGHT: u16 = 6;
pub const HEAD_BINS: u16 = 30;
pub const POINT_BASELINE_MIN: f32 = 2.0;
pub const POINT_BASELINE_MAX: f32 = 120.0;
pub const POINT_BASELINE_STEP: f32 = 0.5;

/// Неизменяемые параметры точки. Считаются один раз и пробрасываются дальше.
/// Базовая линия сюда не входит: она шаг предобработки кадра.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StaticPointParams {
    /// предполагаемая HV точки
    pub hv: f32,

    /// Время начала набора точки
    pub start_time: NaiveDateTime,

    /// время набора точки; на проводе — число в секундах (базовая единица uom)
    pub acquisition_time: Time,

    /// длина кадра; на проводе — число в секундах (базовая единица uom)
    pub frame_len: Time,

    /// номера блоков, которые нужно исключить из анализа
    /// размер блока равен [CUTOFF_BIN_SIZE](crate::preprocess::CUTOFF_BIN_SIZE)
    pub bad_blocks: BTreeSet<usize>,
}

impl StaticPointParams {
    /// Извлекает параметры точки из метаданных и кадра.
    pub fn from_point(meta: Option<NumassMeta>, point: &rsb_event::Point) -> Self {
        // метаданные хранят время набора в секундах
        use crate::types::second;

        let (acquisition_time, start_time) = if let Some(NumassMeta::Reply(Reply::AcquirePoint {
            acquisition_time,
            start_time,
            ..
        })) = meta
        {
            (Time::new::<second>(f64::from(acquisition_time)), start_time)
        } else {
            panic!("acquisition_time/start_time not found in metadata")
        };

        let hv = if let Some(NumassMeta::Reply(Reply::AcquirePoint {
            external_meta:
                Some(ExternalMeta {
                    hv1_value: Some(hv),
                    ..
                }),
            ..
        })) = meta
        {
            hv
        } else {
            -1.0 // TODO: fix this!
        };

        let bad_blocks = if hv > CHECK_HV_THRESHOLD {
            BTreeSet::new()
        } else {
            // ось гистограммы плотности триггеров — наносекунды
            let acquisition_ns = time_ns(acquisition_time);
            let mut trigger_density_local =
                PointHistogram::new_step(0.0..acquisition_ns as f32, CHECK_BIN_SIZE as f32);

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
                    if (idx + 1) as u64 * CHECK_BIN_SIZE <= acquisition_ns && *count == 0.0 {
                        let block_idx = (idx as u64 * CHECK_BIN_SIZE) / CUTOFF_BIN_SIZE;
                        bad_blocks.insert(block_idx as usize);
                    }
                });

            bad_blocks
        };

        let frame_len = bin_time(
            point
                .channels
                .first()
                .unwrap()
                .blocks
                .first()
                .unwrap()
                .frames
                .first()
                .unwrap()
                .data
                .len()
                / 2,
        );

        Self {
            acquisition_time,
            start_time,
            frame_len,
            hv,
            bad_blocks,
        }
    }

    /// Эффективное время набора точки: [`acquisition_time`](Self::acquisition_time)
    /// без вырезанных плохих блоков.
    pub fn effective_time(&self) -> Time {
        let removed_ns = self.bad_blocks.len() as u64 * CUTOFF_BIN_SIZE;
        self.acquisition_time - nanoseconds(removed_ns as f64)
    }
}

/// Оценка базовой линии, которую вычитают из кадра.
#[cfg_attr(
    feature = "stubgen",
    pyo3_stub_gen::derive::gen_stub_pyclass_complex_enum
)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(eq, module = "numass_processing", from_py_object)
)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Baseline {
    /// Константа на канал, задаёт пользователь. Индекс — номер канала.
    Constant([f32; 7]),
    /// Среднее первых `bins` отсчётов этого кадра. Короткий кадр берётся целиком.
    Head { bins: u16 },
    /// Пик гистограммы по всем отсчётам канала на уже обработанных кадрах точки.
    Point { min: f32, max: f32, step: f32 },
}

/// Один шаг предобработки. Список выполняется по порядку, каждый шаг — один проход.
///
/// `Invert` — пустая структура: PyO3 не принимает unit-вариант в complex enum.
/// В JSON это объект `{"Invert":{}}`.
#[cfg_attr(
    feature = "stubgen",
    pyo3_stub_gen::derive::gen_stub_pyclass_complex_enum
)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(eq, module = "numass_processing", from_py_object)
)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PreprocessStep {
    /// FIR-фильтр с окном `left + center + right`.
    Fir { left: u16, center: u16, right: u16 },
    /// Вычесть базовую линию.
    Baseline(Baseline),
    /// Инвертировать сигнал.
    Invert {},
    /// Слить вейвформы всех каналов кадра в одну: отсчёты складываются
    /// бин за бин. Итог остаётся под номером наименьшего канала кадра.
    Merge {},
    /// Luau-скрипт. Мутирует тот же кадр, что FIR и baseline.
    ///
    /// Скрипт возвращает `function(frame)`. На время вызова доступны
    /// глобалы `point` (таблица `StaticPointParams` или `nil`, если
    /// метаданных нет) и `host` (таблица функций хоста `ScriptHost`).
    /// Формат кадра описан в `AVERAGE_SCRIPT`.
    Script { source: String },
}

/// Текст скрипта для шага [`PreprocessStep::Script`] — аналог
/// [`PreprocessParams::average`].
///
/// Скрипт вычитает среднее первых [`HEAD_BINS`] отсчётов и инвертирует
/// сигнал. Арифметика выполняется в Luau, без отдельной обвязки на каждую
/// операцию.
///
/// Формат кадра:
/// - ключ — id канала, 0-based, как в остальном пайплайне;
/// - `frame[ch].samples` — массив, индексация с 1, `#` возвращает длину;
/// - `frame[ch].delay` — сдвиг относительно сырого кадра.
///
/// Отсчёты записываются в таблицу на время вызова и после него читаются
/// обратно в тот же кадр.
///
/// Глобал `point` содержит поля `hv`, `acquisition_time`, `frame_len`
/// (в секундах), `start_time` и `bad_blocks` — список номеров блоков.
/// Без метаданных точки `point` равен `nil`; изменения `point` в скрипте
/// не возвращаются в Rust.
///
/// Глобал `host` содержит функции, зарегистрированные при сборке VM
/// (см. [`ScriptHost`](crate::script::ScriptHost)).
pub const AVERAGE_SCRIPT: &str = r#"-- Аналог PreprocessParams::average: Baseline::Head + Invert.
-- HEAD_BINS совпадает с константой Rust. Если отсчётов в кадре меньше
-- HEAD_BINS, среднее считается по всем отсчётам.
local HEAD_BINS = 30

-- frame — таблица кадра с индексацией отсчётов от 1.
-- point и host — глобалы.
return function(frame)
    for _, channel in frame do
        local samples = channel.samples
        local count = math.min(HEAD_BINS, #samples)
        local sum = 0
        for index = 1, count do
            sum += samples[index]
        end
        local baseline = if count > 0 then sum / count else 0

        -- Вычесть голову и инвертировать: -(sample - baseline).
        for index = 1, #samples do
            samples[index] = baseline - samples[index]
        end
    end
end
"#;

/// Упорядоченный список шагов waveform → waveform.
#[cfg_attr(feature = "stubgen", pyo3_stub_gen::derive::gen_stub_pyclass)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(eq, module = "numass_processing", get_all, set_all, from_py_object)
)]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreprocessParams {
    pub steps: Vec<PreprocessStep>,
}

impl Default for PreprocessParams {
    /// Цепочка новой версии. От алгоритма process не зависит.
    fn default() -> Self {
        Self::average()
    }
}

impl PreprocessParams {
    pub fn empty() -> Self {
        Self { steps: Vec::new() }
    }

    /// FIR + пик по точке. Сохраняет числа прежнего Trapezoid.
    pub fn trapezoid() -> Self {
        Self {
            steps: vec![Self::fir_step(), Self::point_step()],
        }
    }

    /// Только FIR, без пика по точке. Для одного живого кадра.
    pub fn trapezoid_fir() -> Self {
        Self {
            steps: vec![Self::fir_step()],
        }
    }

    /// Голова кадра + инверсия. Сохраняет числа прежнего Average.
    pub fn average() -> Self {
        Self {
            steps: vec![
                PreprocessStep::Baseline(Baseline::Head { bins: HEAD_BINS }),
                PreprocessStep::Invert {},
            ],
        }
    }

    fn fir_step() -> PreprocessStep {
        PreprocessStep::Fir {
            left: FIR_LEFT,
            center: FIR_CENTER,
            right: FIR_RIGHT,
        }
    }

    fn point_step() -> PreprocessStep {
        PreprocessStep::Baseline(Baseline::Point {
            min: POINT_BASELINE_MIN,
            max: POINT_BASELINE_MAX,
            step: POINT_BASELINE_STEP,
        })
    }
}

/// Канал после предобработки.
///
/// `delay` — сдвиг индекса относительно сырого кадра. FIR записывает сюда
/// `left + center + right`, как прежний `offset`.
#[derive(Clone, Debug)]
pub struct ChannelSamples {
    pub samples: Vec<f32>,
    pub delay: usize,
}

pub type ProcessedFrame = BTreeMap<u8, ChannelSamples>;
pub type ProcessedPoint = BTreeMap<u64, ProcessedFrame>;

/// Материализует сырые кадры в `f32` и проводит каждый шаг ровно один раз.
///
/// Скриптовый шаг видит `point == nil`. Для прогона с метаданными —
/// [`apply_waveforms_with`].
pub fn apply_waveforms(
    waveforms: &NumassWaveformsFast<'_>,
    steps: &[PreprocessStep],
) -> ProcessedPoint {
    apply_waveforms_with(waveforms, steps, None)
}

/// Как [`apply_waveforms`], но скрипт получает параметры точки.
///
/// Точка раскладывается в таблицу один раз на прогон, не на кадр.
pub fn apply_waveforms_with(
    waveforms: &NumassWaveformsFast<'_>,
    steps: &[PreprocessStep],
    point: Option<&StaticPointParams>,
) -> ProcessedPoint {
    let mut buffers = waveforms
        .iter()
        .map(|(time, frame)| (*time, materialize(frame)))
        .collect::<ProcessedPoint>();
    apply_steps(&mut buffers, steps, point);
    buffers
}

/// Та же цепочка на одном кадре. `Baseline::Point` видит только этот кадр.
/// Скрипт видит `point == nil`.
pub fn apply_frame(frame: &NumassFrameFast<'_>, steps: &[PreprocessStep]) -> ProcessedFrame {
    let mut buffers = BTreeMap::from([(0, materialize(frame))]);
    apply_steps(&mut buffers, steps, None);
    buffers.remove(&0).unwrap()
}

fn materialize(frame: &NumassFrameFast<'_>) -> ProcessedFrame {
    frame
        .iter()
        .map(|(channel, waveform)| {
            (
                *channel,
                ChannelSamples {
                    samples: waveform.iter().map(|sample| *sample as f32).collect(),
                    delay: 0,
                },
            )
        })
        .collect()
}

fn apply_steps(
    buffers: &mut ProcessedPoint,
    steps: &[PreprocessStep],
    point: Option<&StaticPointParams>,
) {
    for step in steps {
        apply_step(buffers, step, point);
    }
}

fn apply_step(
    buffers: &mut ProcessedPoint,
    step: &PreprocessStep,
    point: Option<&StaticPointParams>,
) {
    match step {
        PreprocessStep::Fir {
            left,
            center,
            right,
        } => {
            for frame in buffers.values_mut() {
                for channel in frame.values_mut() {
                    apply_fir(channel, *left as usize, *center as usize, *right as usize);
                }
            }
        }
        PreprocessStep::Baseline(Baseline::Constant(values)) => {
            for frame in buffers.values_mut() {
                for (channel, samples) in frame {
                    let baseline = values.get(*channel as usize).copied().unwrap_or(0.0);
                    for sample in &mut samples.samples {
                        *sample -= baseline;
                    }
                }
            }
        }
        PreprocessStep::Baseline(Baseline::Head { bins }) => {
            let bins = *bins as usize;
            for frame in buffers.values_mut() {
                for channel in frame.values_mut() {
                    let count = bins.min(channel.samples.len());
                    if count == 0 {
                        continue;
                    }
                    let mean = channel.samples[..count].iter().sum::<f32>() / count as f32;
                    for sample in &mut channel.samples {
                        *sample -= mean;
                    }
                }
            }
        }
        PreprocessStep::Baseline(Baseline::Point { min, max, step }) => {
            let peaks = histogram_peaks(&samples_histogram(buffers, *min, *max, *step));
            for frame in buffers.values_mut() {
                for (channel, samples) in frame {
                    let baseline = peaks.get(*channel as usize).copied().unwrap_or(0.0);
                    for sample in &mut samples.samples {
                        *sample -= baseline;
                    }
                }
            }
        }
        PreprocessStep::Invert {} => {
            for frame in buffers.values_mut() {
                for channel in frame.values_mut() {
                    for sample in &mut channel.samples {
                        *sample = -*sample;
                    }
                }
            }
        }
        PreprocessStep::Merge {} => {
            for frame in buffers.values_mut() {
                merge_frame(frame);
            }
        }
        // VM создаётся на один вызов шага и остаётся в текущем потоке.
        // Хост не хранит состояния, поэтому синхронизация между точками
        // не требуется.
        PreprocessStep::Script { source } => {
            let runtime = ScriptRuntime::compile(source, &PreprocessHost)
                .unwrap_or_else(|err| panic!("preprocess script: {err}"));
            runtime
                .apply(buffers, point)
                .unwrap_or_else(|err| panic!("preprocess script: {err}"));
        }
    }
}

/// Складывает вейвформы всех каналов кадра в одну, бин за бин.
///
/// Итог хранится под номером наименьшего канала кадра: этот канал есть
/// и в сырых кодах, поэтому дальнейшая обработка (`Extract`) не теряет кадр.
/// `delay` берётся у первого канала: шаги не различают каналы, внутри
/// одного кадра он совпадает.
fn merge_frame(frame: &mut ProcessedFrame) {
    let channels = std::mem::take(frame);
    let mut merged: Option<(u8, ChannelSamples)> = None;
    for (channel, samples) in channels {
        match &mut merged {
            None => merged = Some((channel, samples)),
            Some((_, acc)) => {
                for (idx, sample) in samples.samples.iter().enumerate() {
                    if idx < acc.samples.len() {
                        acc.samples[idx] += *sample;
                    } else {
                        acc.samples.push(*sample);
                    }
                }
            }
        }
    }
    if let Some((channel, samples)) = merged {
        frame.insert(channel, samples);
    }
}

fn apply_fir(channel: &mut ChannelSamples, left: usize, center: usize, right: usize) {
    let width = left + center + right;
    if width == 0 {
        return;
    }
    channel.samples = emulate_fir(&channel.samples, right, center, left);
    channel.delay += width;
}

pub fn emulate_fir(waveform: &[f32], right: usize, center: usize, left: usize) -> Vec<f32> {
    let width = left + center + right;
    if width == 0 {
        return Vec::new();
    }
    waveform
        .windows(width)
        .map(|window| {
            let right_sum: f32 = window[left + center..].iter().copied().sum();
            let left_sum: f32 = window[..left].iter().copied().sum();
            (right_sum - left_sum) / (left + right) as f32
        })
        .collect()
}

/// Гистограмма всех отсчётов уже материализованных кадров.
pub fn samples_histogram(frames: &ProcessedPoint, min: f32, max: f32, step: f32) -> PointHistogram {
    let mut histogram = PointHistogram::new_step(min..max, step);
    for frame in frames.values() {
        for (channel, samples) in frame {
            histogram.add_batch(*channel, samples.samples.clone());
        }
    }
    histogram
}

/// Пик гистограммы по каналам 0..6. Пустой канал остаётся 0.
pub fn histogram_peaks(histogram: &PointHistogram) -> [f32; 7] {
    let mut peaks = [0.0; 7];
    for (channel, counts) in &histogram.channels {
        if *channel >= 7 {
            continue;
        }
        let mut max_idx = 0;
        for (idx, count) in counts.iter().enumerate() {
            if *count > counts[max_idx] {
                max_idx = idx;
            }
        }
        peaks[*channel as usize] = histogram.x[max_idx];
    }
    peaks
}

pub fn frame_to_waveform(frame: &Frame) -> &[i16] {
    unsafe { std::slice::from_raw_parts(frame.data.as_ptr() as *const i16, frame.data.len() / 2) }
}

/// remap waveforms from protobuf message to more convenient format (no copy).
pub fn extract_waveforms(point: &rsb_event::Point) -> NumassWaveformsFast<'_> {
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

#[cfg(feature = "egui")]
pub fn draw_raw_frame(frame: &NumassFrameFast<'_>, ui: &mut PlotUi) {
    for (channel, waveform) in frame {
        let points: Vec<[f64; 2]> = ProcessedWaveform::from(*waveform).into();
        ui.line(
            Line::new(format!("ch #{}", channel + 1), points)
                .color(color_for_index(*channel as usize))
                .style(LineStyle::Dashed { length: 10.0 }),
        );
    }
}

#[cfg(feature = "egui")]
pub fn draw_processed_frame(frame: &ProcessedFrame, ui: &mut PlotUi) {
    for (channel, samples) in frame {
        let points = samples
            .samples
            .iter()
            .enumerate()
            .map(|(idx, amplitude)| {
                [
                    bin_time(idx + samples.delay).get::<nanosecond>(),
                    *amplitude as f64,
                ]
            })
            .collect::<Vec<_>>();
        ui.line(
            Line::new(format!("pre ch #{}", channel + 1), points)
                .color(color_for_index(*channel as usize)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_serde_uses_base_unit_seconds() {
        use crate::types::second;

        let preprocess = StaticPointParams {
            hv: 16e3,
            start_time: chrono::DateTime::from_timestamp(0, 0).unwrap().naive_utc(),
            acquisition_time: Time::new::<second>(30.0),
            frame_len: bin_time(4096),
            bad_blocks: BTreeSet::new(),
        };

        // uom сериализует время числом в базовой единице — секундах
        let value = serde_json::to_value(&preprocess).unwrap();
        assert_eq!(value["acquisition_time"], serde_json::json!(30.0));
        assert_eq!(
            value["frame_len"],
            serde_json::json!(bin_time(4096).get::<second>())
        );

        let decoded: StaticPointParams = serde_json::from_value(value).unwrap();
        assert_eq!(
            time_ns(decoded.acquisition_time),
            time_ns(preprocess.acquisition_time)
        );
        assert_eq!(time_ns(decoded.frame_len), time_ns(preprocess.frame_len));
    }

    fn one_frame(waveform: &[i16]) -> (NumassWaveformsFast<'_>, ProcessedFrame) {
        let frame = BTreeMap::from([(0u8, waveform)]);
        let waveforms = BTreeMap::from([(0u64, frame.clone())]);
        let processed = apply_waveforms(&waveforms, &PreprocessParams::average().steps);
        (waveforms, processed.into_iter().next().unwrap().1)
    }

    #[test]
    fn head_then_invert_compensates_negative_pulse() {
        let mut waveform = vec![100i16; 30];
        waveform.extend([80i16; 10]);
        let (_, processed) = one_frame(&waveform);
        let samples = &processed[&0].samples;

        assert!(samples[..30].iter().all(|value| value.abs() < 1e-4));
        assert!(
            samples[30..]
                .iter()
                .all(|value| (*value - 20.0).abs() < 1e-4)
        );
        assert_eq!(samples.iter().sum::<f32>() / samples.len() as f32, 5.0);
    }

    #[test]
    fn short_frame_head_uses_every_bin() {
        let waveform = [2i16, 4, 6, 8];
        let (_, processed) = one_frame(&waveform);
        let mean = processed[&0].samples.iter().sum::<f32>() / 4.0;
        assert!(mean.abs() < 1e-4);
    }

    #[test]
    fn point_peak_does_not_need_an_algorithm() {
        let waveform = vec![10i16; 20];
        let frame = BTreeMap::from([(0u8, waveform.as_slice())]);
        let waveforms = BTreeMap::from([(0u64, frame)]);
        let steps = [PreprocessStep::Baseline(Baseline::Point {
            min: POINT_BASELINE_MIN,
            max: POINT_BASELINE_MAX,
            step: POINT_BASELINE_STEP,
        })];
        let processed = apply_waveforms(&waveforms, &steps);
        // бин (10 - 2) / 0.5 = 16, центр бина 10.25
        let expected = 10.0 - 10.25;
        assert!(
            processed[&0][&0]
                .samples
                .iter()
                .all(|value| (*value - expected).abs() < 1e-3)
        );
    }

    #[test]
    fn fir_runs_once_and_point_sees_filtered_samples() {
        let waveform: Vec<i16> = (0..30).map(|idx| (idx * 40) as i16).collect();
        let frame = BTreeMap::from([(0u8, waveform.as_slice())]);
        let waveforms = BTreeMap::from([(1u64, frame)]);
        let steps = [
            PreprocessStep::Fir {
                left: 1,
                center: 0,
                right: 1,
            },
            PreprocessStep::Baseline(Baseline::Point {
                min: POINT_BASELINE_MIN,
                max: POINT_BASELINE_MAX,
                step: POINT_BASELINE_STEP,
            }),
        ];

        let processed = apply_waveforms(&waveforms, &steps);
        let channel = &processed[&1][&0];
        let once = emulate_fir(
            &waveform
                .iter()
                .map(|value| *value as f32)
                .collect::<Vec<_>>(),
            1,
            0,
            1,
        );

        assert_eq!(channel.delay, 2);
        assert_eq!(channel.samples.len(), once.len());
        assert!(once.iter().all(|value| (*value - 20.0).abs() < 1e-4));
        assert!(
            channel
                .samples
                .iter()
                .all(|value| (*value - (20.0 - 20.25)).abs() < 1e-3)
        );
    }

    #[test]
    fn merge_sums_channels_binwise() {
        let first = [1i16, 2, 3];
        let second = [10i16, 20, 30];
        let frame = BTreeMap::from([(0u8, first.as_slice()), (1u8, second.as_slice())]);
        let waveforms = BTreeMap::from([(0u64, frame)]);
        let processed = apply_waveforms(&waveforms, &[PreprocessStep::Merge {}]);

        let frame = &processed[&0];
        assert_eq!(frame.len(), 1);
        assert_eq!(frame[&0].samples, vec![11.0, 22.0, 33.0]);
    }

    #[test]
    fn merge_keeps_delay_and_smallest_channel() {
        let waveform: Vec<i16> = (0..30).map(|idx| (idx * 40) as i16).collect();
        let frame = BTreeMap::from([(5u8, waveform.as_slice()), (2u8, waveform.as_slice())]);
        let waveforms = BTreeMap::from([(1u64, frame)]);
        let steps = [
            PreprocessStep::Fir {
                left: 1,
                center: 0,
                right: 1,
            },
            PreprocessStep::Merge {},
        ];

        let processed = apply_waveforms(&waveforms, &steps);

        let frame = &processed[&1];
        assert_eq!(frame.len(), 1);
        let merged = &frame[&2];
        assert_eq!(merged.delay, 2);
        assert_eq!(merged.samples.len(), 29);
        // FIR даёт 20.0 на канал, каналов два
        assert!(
            merged
                .samples
                .iter()
                .all(|value| (*value - 40.0).abs() < 1e-4)
        );
    }

    #[test]
    fn script_matches_head_then_invert() {
        let mut waveform = vec![100i16; 30];
        waveform.extend([80i16; 10]);
        let frame = BTreeMap::from([(0u8, waveform.as_slice())]);
        let waveforms = BTreeMap::from([(0u64, frame)]);

        let scripted = apply_waveforms(
            &waveforms,
            &[PreprocessStep::Script {
                source: AVERAGE_SCRIPT.to_owned(),
            }],
        );
        let builtin = apply_waveforms(&waveforms, &PreprocessParams::average().steps);

        assert_eq!(scripted[&0][&0].samples, builtin[&0][&0].samples);
        assert_eq!(scripted[&0][&0].delay, builtin[&0][&0].delay);
    }

    #[test]
    fn script_sees_point_and_writes_back_frame() {
        use crate::types::second;

        let waveform = [1i16, 2, 3, 4];
        let frame = BTreeMap::from([(1u8, waveform.as_slice())]);
        let waveforms = BTreeMap::from([(7u64, frame)]);
        let mut bad_blocks = BTreeSet::new();
        bad_blocks.insert(3);
        let point = StaticPointParams {
            hv: 14_000.0,
            start_time: chrono::DateTime::from_timestamp(0, 0).unwrap().naive_utc(),
            acquisition_time: Time::new::<second>(20.0),
            frame_len: bin_time(4),
            bad_blocks,
        };
        // `point` — обычная таблица, собранная один раз на вызов. Тест
        // проверяет её поля и запись в отсчёты канала.
        let source = r#"
            return function(frame)
                assert(point.hv == 14000)
                assert(point.acquisition_time == 20)
                assert(point.bad_blocks[1] == 3)
                local channel = frame[1]
                channel.samples[1] = point.hv
            end
        "#;

        let processed = apply_waveforms_with(
            &waveforms,
            &[PreprocessStep::Script {
                source: source.to_owned(),
            }],
            Some(&point),
        );

        let samples = &processed[&7][&1].samples;
        assert_eq!(samples[0], 14_000.0);
        assert_eq!(&samples[1..], &[2.0, 3.0, 4.0]);
    }
}
