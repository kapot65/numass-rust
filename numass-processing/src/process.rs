//! # Processing
//! This module contains a bunch of built-in processing algorithms
//! (extraction events from waveforms)
//! see [params](crate::process::ProcessParams) for details.
//!

use std::collections::BTreeMap;

#[cfg(feature = "egui")]
use {
    crate::utils::color_for_index,
    egui::Color32,
    egui_plot::{HLine, Line, MarkerShape, PlotUi, Points, VLine},
};

use numass::{NumassMeta, protos::rsb_event};
use serde::{Deserialize, Serialize};

use crate::{
    constants::{
        KEV_COEFF_AVERAGE, KEV_COEFF_FIRST_PEAK, KEV_COEFF_LIKHOVID, KEV_COEFF_LONGDIFF,
        KEV_COEFF_MAX, KEV_COEFF_TRAPEZIOD,
    },
    preprocess::{
        Preprocess, PreprocessParams, ProcessedFrame, apply_waveforms, extract_waveforms,
    },
    types::{FrameEvent, NumassEvent, NumassEvents, NumassFrameFast},
};

#[repr(C)]
#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
pub struct HWResetParams {
    #[cfg_attr(feature = "clap", arg(id = "reset-window", long = "reset-window", default_value_t = HW_RESET_WINDOW))]
    pub window: u16,
    #[cfg_attr(feature = "clap", arg(id = "reset-treshold", long = "reset-treshold", default_value_t = HW_RESET_TRESHOLD))]
    pub treshold: i16,
    #[cfg_attr(feature = "clap", arg(id = "reset-size", long = "reset-size", default_value_t = HW_RESET_SIZE))]
    pub size: u16,
}

#[repr(C)]
#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum SkipOption {
    None, // TODO: use rust Option<T> instead?
    Bad,
    Good,
}

#[repr(C)]
/// Built-in algorithms params for processing the data.
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize, Hash)]
#[cfg_attr(feature = "clap", derive(clap::Subcommand))]
pub enum Algorithm {
    Max,
    Likhovid {
        #[cfg_attr(feature = "clap", arg(long, default_value_t = LIKHOVID_LEFT))]
        left: u16,
        #[cfg_attr(feature = "clap", arg(long, default_value_t = LIKHOVID_RIGHT))]
        right: u16,
    },
    FirstPeak {
        #[cfg_attr(feature = "clap", arg(long, default_value_t = FIRSTPEAK_THRESHOLD))]
        threshold: i16,
        #[cfg_attr(feature = "clap", arg(long, default_value_t = FIRSTPEAK_LEFT))]
        left: u16,
    },
    Trapezoid {
        #[cfg_attr(feature = "clap", arg(long, default_value_t = TRAPEZOID_TRESHOLD))]
        treshold: i16,
        #[cfg_attr(feature = "clap", arg(long, default_value_t = TRAPEZOID_MIN_LENGTH))]
        min_length: u16,
        #[cfg_attr(feature = "clap", arg(long, value_enum, default_value = "none"))]
        skip: SkipOption,
        #[cfg_attr(feature = "clap", command(flatten))]
        reset_detection: HWResetParams,
    },
    LongDiff {
        #[cfg_attr(feature = "clap", command(flatten))]
        reset_detection: HWResetParams,
    },
    Average,
}

// Scalar defaults shared between the `*_DEFAULT` singletons and the clap
// `default_value_t` attributes, so each number is defined in exactly one place.
pub const LIKHOVID_LEFT: u16 = 15;
pub const LIKHOVID_RIGHT: u16 = 36;
pub const FIRSTPEAK_THRESHOLD: i16 = 10;
pub const FIRSTPEAK_LEFT: u16 = 8;
pub const TRAPEZOID_TRESHOLD: i16 = 16;
pub const TRAPEZOID_MIN_LENGTH: u16 = 16;
pub const HW_RESET_WINDOW: u16 = 10;
pub const HW_RESET_TRESHOLD: i16 = 800;
pub const HW_RESET_SIZE: u16 = 110;

pub const LIKHOVID_DEFAULT: Algorithm = Algorithm::Likhovid {
    left: LIKHOVID_LEFT,
    right: LIKHOVID_RIGHT,
};
pub const FIRSTPEAK_DEFAULT: Algorithm = Algorithm::FirstPeak {
    threshold: FIRSTPEAK_THRESHOLD,
    left: FIRSTPEAK_LEFT,
};
pub const TRAPEZOID_DEFAULT: Algorithm = Algorithm::Trapezoid {
    treshold: TRAPEZOID_TRESHOLD,
    min_length: TRAPEZOID_MIN_LENGTH,
    skip: SkipOption::None,
    reset_detection: HWResetParams {
        window: HW_RESET_WINDOW,
        treshold: HW_RESET_TRESHOLD,
        size: HW_RESET_SIZE,
    },
};
pub const LONGDIFF_DEFAULT: Algorithm = Algorithm::LongDiff {
    reset_detection: HWResetParams {
        window: HW_RESET_WINDOW,
        treshold: HW_RESET_TRESHOLD,
        size: HW_RESET_SIZE,
    },
};

impl Default for Algorithm {
    fn default() -> Self {
        TRAPEZOID_DEFAULT
    }
}

#[repr(C)]
/// Built-in processing params.
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize, Hash)]
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

/// Стандартизованный кадр и сырые коды АЦП одного триггера.
pub struct FrameIn<'a> {
    pub processed: &'a ProcessedFrame,
    pub raw: &'a NumassFrameFast<'a>,
}

/// Извлечение событий из уже стандартизованного кадра.
pub trait Extract {
    fn extract(
        &self,
        input: &FrameIn<'_>,
        #[cfg(feature = "egui")] ui: &mut Option<&mut PlotUi>,
    ) -> Vec<NumassEvent>;

    fn to_kev(&self, amplitude: f32, channel: u8) -> f32;
}

impl Extract for Algorithm {
    fn extract(
        &self,
        input: &FrameIn<'_>,
        #[cfg(feature = "egui")] ui: &mut Option<&mut PlotUi>,
    ) -> Vec<NumassEvent> {
        match self {
            Algorithm::Max => extract_max(input),
            Algorithm::Likhovid { left, right } => extract_likhovid(input, *left, *right),
            Algorithm::FirstPeak { threshold, left } => {
                extract_first_peak(input, *threshold, *left)
            }
            Algorithm::Trapezoid {
                treshold,
                min_length,
                skip,
                reset_detection,
            } => extract_trapezoid(
                input,
                *treshold,
                *min_length,
                skip,
                reset_detection,
                #[cfg(feature = "egui")]
                ui,
            ),
            Algorithm::LongDiff { reset_detection } => extract_long_diff(
                input,
                reset_detection,
                #[cfg(feature = "egui")]
                ui,
            ),
            Algorithm::Average => extract_average(input),
        }
    }

    fn to_kev(&self, amplitude: f32, channel: u8) -> f32 {
        convert_to_kev(&amplitude, channel, self)
    }
}

/// Built-in processing algorithm.
/// Do not use this function directly without reason, use [process_point](crate::storage::process_point) instead.
pub fn extract_events(
    meta: Option<NumassMeta>,
    point: rsb_event::Point,
    preprocess: &PreprocessParams,
    process: &ProcessParams,
) -> (NumassEvents, Preprocess) {
    let meta_params = Preprocess::from_point(meta, &point);
    let raw = extract_waveforms(&point);
    let processed = apply_waveforms(&raw, &preprocess.steps);

    let events = raw
        .iter()
        .map(|(time, frame)| {
            let mut events = extract_frame(
                &processed[time],
                frame,
                &process.algorithm,
                #[cfg(feature = "egui")]
                &mut None,
            );
            if process.convert_to_kev {
                events.iter_mut().for_each(|(_, event)| {
                    if let FrameEvent::Event {
                        amplitude, channel, ..
                    } = event
                    {
                        *amplitude = process.algorithm.to_kev(*amplitude, *channel);
                    }
                });
            }
            (*time, events)
        })
        .collect::<BTreeMap<_, _>>();

    (events, meta_params)
}

/// События одного кадра. Общие маркеры событий рисует эта обёртка, не тело алгоритма.
pub fn extract_frame(
    processed: &ProcessedFrame,
    raw: &NumassFrameFast<'_>,
    algorithm: &impl Extract,
    #[cfg(feature = "egui")] ui: &mut Option<&mut PlotUi>,
) -> Vec<NumassEvent> {
    let input = FrameIn { processed, raw };
    let mut events = algorithm.extract(
        &input,
        #[cfg(feature = "egui")]
        ui,
    );
    events.sort_by_key(|(pos, _)| *pos);

    #[cfg(feature = "egui")]
    if let Some(ui) = ui {
        draw_process_events(&events, ui);
    }

    events
}

/// Built-in keV convertion (according to crate::constants).
/// TODO: make configurable
pub fn convert_to_kev(amplitude: &f32, ch_id: u8, algorithm: &Algorithm) -> f32 {
    let [a, b] = match algorithm {
        Algorithm::Max => KEV_COEFF_MAX[ch_id as usize],
        Algorithm::Likhovid { .. } => KEV_COEFF_LIKHOVID[ch_id as usize],
        Algorithm::FirstPeak { .. } => KEV_COEFF_FIRST_PEAK[ch_id as usize],
        Algorithm::Trapezoid { .. } => KEV_COEFF_TRAPEZIOD[ch_id as usize],
        Algorithm::LongDiff { .. } => KEV_COEFF_LONGDIFF[ch_id as usize],
        Algorithm::Average => KEV_COEFF_AVERAGE[ch_id as usize],
    };
    a * *amplitude + b
}

/// Каналы, которые есть и в обработанном кадре, и в сырых кодах.
///
/// Кортеж: номер канала, отсчёты `f32`, `delay`, сырой сигнал `i16`.
/// `'a` — время жизни данных кадра, а не короткой ссылки на [`FrameIn`].
fn channels<'a>(input: &FrameIn<'a>) -> Vec<(u8, &'a [f32], usize, &'a [i16])> {
    input
        .processed
        .iter()
        .filter_map(|(channel, samples)| {
            input
                .raw
                .get(channel)
                .map(|raw| (*channel, samples.samples.as_slice(), samples.delay, *raw))
        })
        .collect()
}

fn extract_max(input: &FrameIn<'_>) -> Vec<NumassEvent> {
    channels(input)
        .into_iter()
        .map(|(channel, samples, delay, _)| {
            let (index, amplitude) = samples
                .iter()
                .enumerate()
                .max_by(|first, second| first.1.partial_cmp(second.1).unwrap())
                .unwrap();
            (
                (index + delay) as u16 * 8,
                FrameEvent::Event {
                    channel,
                    amplitude: *amplitude,
                    size: 1,
                },
            )
        })
        .collect()
}

fn extract_likhovid(input: &FrameIn<'_>, left: u16, right: u16) -> Vec<NumassEvent> {
    let left = left as usize;
    let right = right as usize;
    channels(input)
        .into_iter()
        .map(|(channel, samples, delay, _)| {
            let (index, _) = samples
                .iter()
                .enumerate()
                .max_by(|first, second| first.1.partial_cmp(second.1).unwrap())
                .unwrap();
            let start = index.saturating_sub(left);
            let end = std::cmp::min(samples.len(), index + right);
            let crop = &samples[start..end];
            let amplitude = crop.iter().sum::<f32>() / crop.len() as f32;
            (
                (index + delay) as u16 * 8,
                FrameEvent::Event {
                    channel,
                    amplitude,
                    size: 1,
                },
            )
        })
        .collect()
}

fn extract_first_peak(input: &FrameIn<'_>, threshold: i16, left: u16) -> Vec<NumassEvent> {
    let left = left as usize;
    channels(input)
        .into_iter()
        .filter_map(|(channel, samples, delay, _)| {
            find_first_peak(samples, threshold as f32).map(|pos| {
                let start = pos.saturating_sub(left);
                let amplitude = samples[start..].iter().sum::<f32>() / 50.0;
                (
                    (pos + delay) as u16 * 8,
                    FrameEvent::Event {
                        channel,
                        amplitude,
                        size: 1,
                    },
                )
            })
        })
        .collect()
}

fn extract_trapezoid(
    input: &FrameIn<'_>,
    treshold: i16,
    min_length: u16,
    skip: &SkipOption,
    reset_detection: &HWResetParams,
    #[cfg(feature = "egui")] ui: &mut Option<&mut PlotUi>,
) -> Vec<NumassEvent> {
    let min_length = min_length as usize;
    let threshold = treshold as f32;
    let reset = detect_reset(input.raw, reset_detection);
    let mut bad_frame = reset.is_some();

    #[cfg(feature = "egui")]
    if let Some(ui) = ui {
        ui.hline(HLine::new("TRIGGER".to_string(), treshold as f64).color(Color32::WHITE));
    }

    let mut events = Vec::new();
    for (channel, samples, delay, raw) in channels(input) {
        if channel == 1
            && let Some((idx, _)) = raw.iter().enumerate().find(|(_, value)| **value == 8189)
        {
            bad_frame = true;
            events.push(overflow_event(channel, idx, raw.len(), reset));
        }
        if channel == 5
            && let Some((idx, _)) = raw.iter().enumerate().find(|(_, value)| **value == 8081)
        {
            bad_frame = true;
            events.push(overflow_event(channel, idx, raw.len(), reset));
        }

        let mut index = 0;
        while index < samples.len() {
            if let Some((reset_start, reset_end)) = reset
                && index + delay == reset_start
            {
                index = reset_end - delay;
                continue;
            }

            if (index == 0 || samples[index - 1] < threshold) && samples[index] >= threshold {
                let mut energy = 0.0;
                let mut event_end = index;
                while event_end < samples.len() && samples[event_end] >= threshold {
                    energy += samples[event_end];
                    event_end += 1;
                    if let Some((reset_start, _)) = reset
                        && event_end + delay == reset_start
                    {
                        break;
                    }
                }

                if event_end - index >= min_length {
                    events.push((
                        (index + delay) as u16 * 8,
                        FrameEvent::Event {
                            channel,
                            amplitude: energy / delay as f32,
                            size: (event_end - index) as u16,
                        },
                    ));
                }
                index = event_end;
                continue;
            }
            index += 1;
        }
    }

    if let Some((reset_start, reset_end)) = reset {
        events.push((
            reset_start as u16 * 8,
            FrameEvent::Reset {
                size: (reset_end - reset_start) as u16,
            },
        ));
    }

    match skip {
        SkipOption::None => events,
        SkipOption::Bad => {
            if bad_frame {
                Vec::new()
            } else {
                events
            }
        }
        SkipOption::Good => {
            if bad_frame {
                events
            } else {
                Vec::new()
            }
        }
    }
}

fn overflow_event(
    channel: u8,
    idx: usize,
    raw_len: usize,
    reset: Option<(usize, usize)>,
) -> NumassEvent {
    let end = reset.map(|(reset_start, _)| reset_start).unwrap_or(raw_len);
    (
        idx as u16 * 8,
        FrameEvent::Overflow {
            channel,
            size: end.abs_diff(idx) as u16,
        },
    )
}

fn extract_long_diff(
    input: &FrameIn<'_>,
    reset_detection: &HWResetParams,
    #[cfg(feature = "egui")] ui: &mut Option<&mut PlotUi>,
) -> Vec<NumassEvent> {
    let reset = detect_reset(input.raw, reset_detection);
    let mut events = Vec::new();
    if reset.is_none() {
        for (channel, samples, _, _) in channels(input) {
            let last_idx = samples.len();
            let head = samples[..12].iter().sum::<f32>() / 12.0;
            let tail = samples[last_idx - 12..last_idx].iter().sum::<f32>() / 12.0;

            #[cfg(feature = "egui")]
            if let Some(ui) = ui {
                ui.line(
                    Line::new(
                        format!("range ch# {}", channel + 1),
                        vec![[0.0, head as f64], [last_idx as f64, head as f64]],
                    )
                    .color(color_for_index(channel as usize))
                    .style(egui_plot::LineStyle::Dashed { length: 10.0 }),
                );
                ui.line(
                    Line::new(
                        format!("range ch# {}", channel + 1),
                        vec![[0.0, tail as f64], [last_idx as f64, tail as f64]],
                    )
                    .color(color_for_index(channel as usize))
                    .style(egui_plot::LineStyle::Dashed { length: 10.0 }),
                );
            }

            events.push((
                0,
                FrameEvent::Event {
                    channel,
                    amplitude: (tail - head) / 4.0,
                    size: last_idx as u16,
                },
            ));
        }
    }

    if let Some((reset_start, reset_end)) = reset {
        events.push((
            reset_start as u16 * 8,
            FrameEvent::Reset {
                size: (reset_end - reset_start) as u16,
            },
        ));
    }
    events
}

fn extract_average(input: &FrameIn<'_>) -> Vec<NumassEvent> {
    channels(input)
        .into_iter()
        .map(|(channel, samples, _, _)| {
            let amplitude = samples.iter().sum::<f32>() / samples.len() as f32;
            (
                0,
                FrameEvent::Event {
                    channel,
                    amplitude,
                    size: samples.len() as u16,
                },
            )
        })
        .collect()
}

fn detect_reset(frame: &NumassFrameFast, params: &HWResetParams) -> Option<(usize, usize)> {
    let HWResetParams {
        window,
        treshold,
        size,
    } = params;

    let window = *window as usize;
    let size = *size as usize;

    let mut reset: Option<(usize, usize)> = None;
    frame.iter().for_each(|(_, waveform)| {
        for i in 0..(waveform.len() - window) {
            if waveform[i] - waveform[i + window] > *treshold {
                if let Some(reset_last) = reset {
                    reset = Some((i.min(reset_last.0), (i + size).max(reset_last.1)));
                } else {
                    reset = Some((i, i + size))
                }
                break;
            }
        }
    });
    reset
}

pub fn find_first_peak<T>(waveform: &[T], threshold: T) -> Option<usize>
where
    T: Copy + PartialOrd,
{
    waveform
        .iter()
        .enumerate()
        .find(|(idx, amp)| {
            let amp = **amp;
            amp > threshold
                && (*idx == 0 || waveform[idx - 1] <= amp)
                && (*idx == waveform.len() - 1 || waveform[idx + 1] <= amp)
        })
        .map(|(idx, _)| idx)
}

#[cfg(feature = "egui")]
fn draw_process_events(events: &[NumassEvent], ui: &mut PlotUi) {
    for (idx, (pos, event)) in events.iter().enumerate() {
        match event {
            FrameEvent::Event {
                channel,
                amplitude,
                size,
            } => {
                let name = format!("ev#{idx} ch# {}", channel + 1);
                let color = color_for_index(*channel as usize);
                ui.vline(VLine::new(name.clone(), *pos as f64 / 8.0).color(color));
                ui.vline(VLine::new(name.clone(), (*pos + *size * 8) as f64 / 8.0).color(color));
                ui.points(
                    Points::new(name, vec![[*pos as f64 / 8.0, *amplitude as f64]])
                        .color(color)
                        .shape(MarkerShape::Diamond)
                        .filled(false)
                        .radius(10.0_f32),
                );
            }
            FrameEvent::Reset { size } => {
                ui.vline(VLine::new("RESET", *pos as f64 / 8.0).color(Color32::WHITE));
                ui.vline(
                    VLine::new("RESET", (*pos + *size * 8) as f64 / 8.0).color(Color32::WHITE),
                );
            }
            FrameEvent::Overflow { channel, size } => {
                let color = color_for_index(*channel as usize);
                let style = egui_plot::LineStyle::Dashed { length: 10.0 };
                ui.vline(
                    VLine::new(format!("OVERFLOW ch# {channel}"), *pos as f64 / 8.0)
                        .color(color)
                        .style(style),
                );
                ui.vline(
                    VLine::new(
                        format!("OVERFLOW ch# {channel}"),
                        (*pos + *size * 8) as f64 / 8.0,
                    )
                    .color(color)
                    .style(style),
                );
            }
            FrameEvent::Frame { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::preprocess::{PreprocessStep, apply_frame};
    use crate::process::{Algorithm, HWResetParams, SkipOption, extract_frame};
    use crate::types::FrameEvent;

    #[test]
    fn average_reads_mean_of_standardized_frame() {
        let samples = vec![0.0f32, 0.0, 20.0, 0.0];
        let processed =
            BTreeMap::from([(0u8, crate::preprocess::ChannelSamples { samples, delay: 0 })]);
        let raw_samples = [0i16; 4];
        let raw = BTreeMap::from([(0u8, raw_samples.as_slice())]);

        let events = extract_frame(
            &processed,
            &raw,
            &Algorithm::Average,
            #[cfg(feature = "egui")]
            &mut None,
        );

        let (pos, event) = &events[0];
        assert_eq!(*pos, 0);
        match event {
            FrameEvent::Event {
                channel,
                amplitude,
                size,
            } => {
                assert_eq!(*channel, 0);
                assert_eq!(*amplitude, 5.0);
                assert_eq!(*size, 4);
            }
            _ => panic!("expected a FrameEvent::Event, got {event:?}"),
        }
    }

    #[test]
    fn trapezoid_position_uses_recorded_delay() {
        let waveform = vec![0i16, 0, 0, 100, 100, 100, 100, 100];
        let frame = BTreeMap::from([(0u8, waveform.as_slice())]);
        let processed = apply_frame(
            &frame,
            &[PreprocessStep::Fir {
                left: 1,
                center: 1,
                right: 1,
            }],
        );

        let events = extract_frame(
            &processed,
            &frame,
            &Algorithm::Trapezoid {
                treshold: 10,
                min_length: 2,
                skip: SkipOption::None,
                reset_detection: HWResetParams {
                    window: 2,
                    treshold: 800,
                    size: 4,
                },
            },
            #[cfg(feature = "egui")]
            &mut None,
        );

        let (pos, event) = events
            .iter()
            .find(|(_, event)| matches!(event, FrameEvent::Event { .. }))
            .unwrap();
        assert_eq!(*pos, (1 + 1 + 1 + 1) * 8);
        match event {
            FrameEvent::Event {
                amplitude, size, ..
            } => {
                assert!((*amplitude - 100.0 / 3.0).abs() < 1e-3);
                assert_eq!(*size, 2);
            }
            _ => unreachable!(),
        }
    }
}
