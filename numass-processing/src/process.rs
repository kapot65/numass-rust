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
    egui_plot::{HLine, Line, PlotUi},
};

use numass::protos::rsb_event;
use serde::{Deserialize, Serialize};

use crate::{
    constants::{
        KEV_COEFF_FIRST_PEAK, KEV_COEFF_LIKHOVID, KEV_COEFF_LONGDIFF, KEV_COEFF_MAX,
        KEV_COEFF_TRAPEZIOD,
    },
    histogram::PointHistogram,
    types::{FrameEvent, NumassEvent, NumassEvents, NumassFrame, NumassWaveforms},
};

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct HWResetParams {
    pub window: usize,
    pub treshold: i16,
    pub size: usize,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub enum SkipOption {
    None,
    Bad,
    Good,
}

/// Built-in algorithms params for processing the data.
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize, Hash)]
pub enum Algorithm {
    Max,
    Likhovid {
        left: usize,
        right: usize,
    },
    FirstPeak {
        threshold: i16,
        left: usize,
    },
    Trapezoid {
        left: usize,
        center: usize,
        right: usize,
        treshold: i16,
        min_length: usize,
        skip: SkipOption,
        reset_detection: HWResetParams,
    },
    LongDiff {
        reset_detection: HWResetParams,
    },
}

/// Неизменяемые параметры, необходимые для обработки кадра
/// могут либо задаваться статично, либо на каждую точку
/// TODO: add default derive
#[derive(Clone)]
pub struct StaticProcessParams {
    pub baseline: Option<Vec<f32>>, // TODO: make more versatile
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
                    (window[left + center..].iter().sum::<i16>()
                        - window[..left].iter().sum::<i16>()) as f32
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

impl StaticProcessParams {
    pub fn from_point(point: &rsb_event::Point, algo: &Algorithm) -> Self {
        // let time = point.channels[0].blocks[0].time;
        Self {
            baseline: Some(baseline_from_point(point, algo)),
        }
    }
}

pub const LIKHOVID_DEFAULT: Algorithm = Algorithm::Likhovid {
    left: 15,
    right: 36,
};
pub const FIRSTPEAK_DEFAULT: Algorithm = Algorithm::FirstPeak {
    threshold: 10,
    left: 8,
};
pub const TRAPEZOID_DEFAULT: Algorithm = Algorithm::Trapezoid {
    left: 6,
    center: 15,
    right: 6,
    treshold: 10,
    min_length: 10,
    skip: SkipOption::None,
    reset_detection: HWResetParams {
        window: 10,
        treshold: 800,
        size: 110,
    },
};
pub const LONGDIFF_DEFAULT: Algorithm = Algorithm::LongDiff {
    reset_detection: HWResetParams {
        window: 10,
        treshold: 800,
        size: 110,
    },
};

impl Default for Algorithm {
    fn default() -> Self {
        Self::FirstPeak {
            threshold: 10,
            left: 8,
        }
    }
}

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

/// remap waveforms from protobuf message to more convenient format (no copy).
pub fn extract_waveforms(point: &rsb_event::Point) -> NumassWaveforms {
    let mut waveforms = BTreeMap::new();

    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = waveforms.entry(frame.time).or_insert(BTreeMap::new());

                let i16_slice = unsafe {
                    std::slice::from_raw_parts(
                        frame.data.as_ptr() as *const i16,
                        frame.data.len() / 2,
                    )
                };

                entry.insert(channel.id as u8, i16_slice);
            }
        }
    }
    waveforms
}

/// Built-in processing algorithm.
/// Function will extract events point wafevorms and keeps its hierarchy.
/// Do not use this function directly without reason, use [process_point](crate::storage::process_point) instead.
pub fn extract_events(point: rsb_event::Point, params: &ProcessParams) -> NumassEvents {
    let (static_params, point) = {
        (
            StaticProcessParams::from_point(&point, &params.algorithm),
            extract_waveforms(&point),
        )
    };

    point
        .into_iter()
        .map(|(time, frame)| {
            let mut events = frame_to_events(
                &frame,
                &params.algorithm,
                &static_params,
                #[cfg(feature = "egui")]
                &mut None,
            );
            if params.convert_to_kev {
                events.iter_mut().for_each(|(_, event)| {
                    if let FrameEvent::Event {
                        amplitude, channel, ..
                    } = event
                    {
                        *amplitude = convert_to_kev(amplitude, *channel, &params.algorithm);
                    }
                });
            }
            (time, events)
        })
        .collect::<BTreeMap<_, _>>()
}

/// Built-in keV convertion (according to crate::constants).
/// TODO: make configurable
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
        Algorithm::Trapezoid { .. } => {
            let [a, b] = KEV_COEFF_TRAPEZIOD[ch_id as usize];
            a * *amplitude + b
        }
        Algorithm::LongDiff { .. } => {
            let [a, b] = KEV_COEFF_LONGDIFF[ch_id as usize];
            a * *amplitude + b
        }
    }
}

/// Extract events from single waveform.
/// Do not use this function directly without reason, use [extract_events](crate::process::extract_events) instead.
/// TODO: add ui argument description
pub fn frame_to_events(
    frame: &NumassFrame,
    algorithm: &Algorithm,
    static_params: &StaticProcessParams,
    #[cfg(feature = "egui")] ui: &mut Option<&mut PlotUi>,
) -> Vec<NumassEvent> {
    let mut events = match algorithm {
        Algorithm::Max => frame
            .iter()
            .map(|(ch_id, waveform)| {
                let (x, y) = waveform
                    .iter()
                    .enumerate()
                    .max_by(|first, second| first.1.partial_cmp(second.1).unwrap())
                    .unwrap();

                (
                    x as u16 * 8,
                    FrameEvent::Event {
                        channel: *ch_id,
                        amplitude: *y as f32,
                        size: 1,
                    },
                )
            })
            .collect::<Vec<_>>(),
        Algorithm::Likhovid { left, right } => frame
            .iter()
            .map(|(ch_id, waveform)| {
                let (x, _) = waveform
                    .iter()
                    .enumerate()
                    .max_by(|first, second| first.1.partial_cmp(second.1).unwrap())
                    .unwrap();

                let amplitude = {
                    let left = if x >= *left { x - left } else { 0 };
                    let right = std::cmp::min(waveform.len(), x + right);
                    let crop = &waveform[left..right];
                    crop.iter().sum::<i16>() as f32 / crop.len() as f32
                };

                (
                    x as u16 * 8,
                    FrameEvent::Event {
                        channel: *ch_id,
                        amplitude,
                        size: 1,
                    },
                )
            })
            .collect::<Vec<_>>(),
        Algorithm::FirstPeak { threshold, left } => {
            frame
                .iter()
                .filter_map(|(ch_id, waveform)| {
                    find_first_peak(waveform, *threshold).map(|pos| {
                        let left = if pos < *left { 0 } else { pos - left };
                        // let length = (waveform.0.len() - pos) as f32;
                        let amplitude = waveform[left..waveform.len()].iter().sum::<i16>();
                        (
                            pos as u16 * 8,
                            FrameEvent::Event {
                                channel: *ch_id,
                                amplitude: amplitude as f32 / 50.0,
                                size: 1,
                            },
                        )
                    })
                })
                .collect::<Vec<_>>()
        }
        Algorithm::Trapezoid {
            left,
            center,
            right,
            treshold,
            min_length,
            skip,
            reset_detection,
        } => {
            let reset = detect_reset(frame, reset_detection);
            let mut bad_frame = reset.is_some();

            #[cfg(feature = "egui")]
            if let Some(ui) = ui {
                ui.hline(
                    HLine::new(*treshold as f64)
                        .color(Color32::WHITE)
                        .name(format!("TRIGGER")),
                );
            }

            let mut events = frame
                .iter()
                .flat_map(|(ch_id, waveform)| {
                    let baseline = if let StaticProcessParams {
                        baseline: Some(baseline),
                    } = static_params
                    {
                        baseline[*ch_id as usize]
                    } else {
                        0.0
                    };

                    let mut events = vec![];

                    if ch_id == &1 {
                        if let Some((idx, _)) =
                            waveform.iter().enumerate().find(|(_, &val)| val == 8189)
                        {
                            let end = if let Some((reset_start, _)) = reset {
                                reset_start
                            } else {
                                waveform.len()
                            };

                            bad_frame = true;

                            events.push((
                                idx as u16 * 8,
                                FrameEvent::Overflow {
                                    channel: *ch_id,
                                    size: end.abs_diff(idx) as u16,
                                },
                            ));
                        }
                    }

                    if ch_id == &5 {
                        if let Some((idx, _)) =
                            waveform.iter().enumerate().find(|(_, &val)| val == 8081)
                        {
                            let end = if let Some((reset_start, _)) = reset {
                                reset_start
                            } else {
                                waveform.len()
                            };

                            bad_frame = true;

                            events.push((
                                idx as u16 * 8,
                                FrameEvent::Overflow {
                                    channel: *ch_id,
                                    size: end.abs_diff(idx) as u16,
                                },
                            ));
                        }
                    }

                    let offset = left + center + right;

                    let filtered = waveform
                        .windows(left + center + right)
                        .map(|window| {
                            (window[left + center..]
                                .iter()
                                .map(|v| *v as i32)
                                .sum::<i32>()
                                - window[..*left].iter().map(|v| *v as i32).sum::<i32>())
                                as f32
                                / (left + right) as f32
                                - baseline
                        })
                        .collect::<Vec<_>>();

                    #[cfg(feature = "egui")]
                    if let Some(ui) = ui {
                        let line = Line::new(
                            filtered
                                .clone()
                                .into_iter()
                                .enumerate()
                                .map(|(idx, amp)| [(idx + offset) as f64, amp as f64])
                                .collect::<Vec<_>>(),
                        )
                        .color(color_for_index(*ch_id as usize))
                        .name(format!("filtered + baseline ch# {}", ch_id + 1))
                        .style(egui_plot::LineStyle::Dashed { length: 10.0 });
                        ui.line(line);
                    };

                    let mut i = 0;
                    while i < filtered.len() {
                        if let Some((reset_start, reset_end)) = reset {
                            if i == reset_start - offset {
                                i = reset_end - offset;
                                continue;
                            }
                        }

                        if (i == 0 || filtered[i - 1] < *treshold as f32)
                            && filtered[i] >= *treshold as f32
                        {
                            let mut energy = 0.0;
                            let mut event_end = i;

                            while event_end < filtered.len()
                                && filtered[event_end] >= *treshold as f32
                            {
                                energy += filtered[event_end];
                                event_end += 1;

                                if let Some((reset_start, _)) = reset {
                                    if event_end == reset_start - offset {
                                        break;
                                    }
                                }
                            }

                            if (event_end - i) >= *min_length {
                                events.push((
                                    (i + offset) as u16 * 8,
                                    FrameEvent::Event {
                                        channel: *ch_id,
                                        amplitude: energy / offset as f32,
                                        size: (event_end - i) as u16,
                                    },
                                ));
                            }

                            i = event_end;
                            continue;
                        }

                        i += 1;
                    }

                    events
                })
                .collect::<Vec<_>>();

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
                        vec![]
                    } else {
                        events
                    }
                }
                SkipOption::Good => {
                    if !bad_frame {
                        vec![]
                    } else {
                        events
                    }
                }
            }

            // events
        }
        Algorithm::LongDiff { reset_detection } => {
            let reset = detect_reset(frame, reset_detection);

            let mut events: Vec<(u16, FrameEvent)> = frame
                .iter()
                .filter_map(|(ch_id, waveform)| {
                    let baseline = if let StaticProcessParams {
                        baseline: Some(baseline),
                    } = static_params
                    {
                        baseline[*ch_id as usize]
                    } else {
                        0.0
                    };

                    if reset.is_some() {
                        return None;
                    }

                    // reset пока не трогаем
                    let last_idx = if let Some((reset_start, _)) = reset {
                        reset_start
                    } else {
                        waveform.len()
                    };

                    let a = waveform[0..12].iter().map(|v| *v as f32).sum::<f32>() / 12.0;
                    let b = waveform[last_idx - 12..last_idx]
                        .iter()
                        .map(|v| *v as f32)
                        .sum::<f32>()
                        / 12.0;
                    let b_pred = a + (baseline / 10.916_667) * (last_idx as f32);

                    #[cfg(feature = "egui")]
                    if let Some(ui) = ui {
                        let a_pred = b - (baseline / 10.916_667) * (last_idx as f32);

                        ui.line(
                            Line::new(
                                vec![[0.0, a as f64], [last_idx as f64, b_pred as f64]], //
                            )
                            .color(color_for_index(*ch_id as usize))
                            .style(egui_plot::LineStyle::Dashed { length: 10.0 })
                            .name(format!("range ch# {}", ch_id + 1)),
                        );

                        ui.line(
                            Line::new(
                                vec![[0.0, a_pred as f64], [last_idx as f64, b as f64]], //
                            )
                            .color(color_for_index(*ch_id as usize))
                            .style(egui_plot::LineStyle::Dashed { length: 10.0 })
                            .name(format!("range ch# {}", ch_id + 1)),
                        )
                    }

                    let amplitude = (b - b_pred) / 4.0;

                    Some((
                        0,
                        FrameEvent::Event {
                            channel: *ch_id,
                            amplitude,
                            size: last_idx as u16,
                        },
                    ))
                })
                .collect::<Vec<_>>();

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
    };

    events.sort_by_key(|(pos, _)| *pos);

    events
}

fn detect_reset(frame: &NumassFrame, params: &HWResetParams) -> Option<(usize, usize)> {
    let HWResetParams {
        window,
        treshold,
        size,
    } = params;
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

pub fn find_first_peak(waveform: &[i16], threshold: i16) -> Option<usize> {
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
