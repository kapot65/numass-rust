//! # Processing
//! This module contains a bunch of built-in processing algorithms
//! (extraction events from waveforms)
//! see [params](crate::process::ProcessParams) for details.
//! 

use std::collections::BTreeMap;

#[cfg(feature = "egui")]
use {
    egui_plot::{HLine, VLine, Line, PlotUi, MarkerShape, Points},
    egui::Color32,
    crate::utils::color_for_index
};

use numass::protos::rsb_event;
use serde::{Deserialize, Serialize};

use crate::{
    constants::{baseline_2024_03, KEV_COEFF_FIRST_PEAK, KEV_COEFF_LIKHOVID, KEV_COEFF_MAX, KEV_COEFF_TRAPEZIOD}, 
    types::{FrameEvent, NumassEvent, NumassEvents, NumassFrame, NumassWaveforms}
};


#[derive(PartialEq, Eq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct HWResetParams {
    pub window: usize,
    pub treshold: i16,
    pub size: usize,
}

/// Built-in algorithms params for processing the data.
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize, Hash)]
pub enum Algorithm {
    Max,
    Likhovid { left: usize, right: usize },
    FirstPeak { threshold: i16, left: usize },
    Trapezoid { 
        left: usize, center: usize, right: usize,
        treshold: i16,
        min_length: usize,
        reset_detection: HWResetParams,
    }
}

/// Неизменяемые параметры, необходимые для обработки кадра
/// могут либо задаваться статично, либо на каждую точку
/// TODO: add default derive
#[derive(Clone)]
pub struct StaticProcessParams {
    pub baseline: Option<Vec<f32>> // TODO: make more versatile
}

impl StaticProcessParams {
    pub fn from_point(point: &rsb_event::Point) -> Self {
        let time = point.channels[0].blocks[0].time;
        Self {
            baseline: Some(vec![
                0.0, // ch1
                baseline_2024_03(time, 1), // ch2
                baseline_2024_03(time, 2), // ch3
                0.0, // ch4
                baseline_2024_03(time, 4), // ch5
                baseline_2024_03(time, 5), // ch6
                baseline_2024_03(time, 6), // ch7
            ])
        }
    }
}

pub const LIKHOVID_DEFAULT: Algorithm = Algorithm::Likhovid { left: 15, right: 36 };
pub const FIRSTPEAK_DEFAULT: Algorithm = Algorithm::FirstPeak { threshold: 10, left: 8 };
pub const TRAPEZOID_DEFAULT: Algorithm = Algorithm::Trapezoid { 
    left: 6, center: 15, right: 6,
    treshold: 27,
    min_length: 10,
    reset_detection: HWResetParams {
        window: 10,
        treshold: 800,
        size: 110
    }
};

impl Default for Algorithm {
    fn default() -> Self {
        Self::FirstPeak { threshold: 10, left: 8 }
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
                    std::slice::from_raw_parts(frame.data.as_ptr() as *const i16, frame.data.len() / 2)
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
        (StaticProcessParams::from_point(&point), extract_waveforms(&point))
    };

    point.into_iter().map(|(time, frame)| {

        let mut events = frame_to_events(&frame, &params.algorithm, &static_params, #[cfg(feature = "egui")] None);
        if params.convert_to_kev {
            events.iter_mut().for_each(|(_, event)| {
                if let FrameEvent::Event { amplitude, channel, .. } = event {
                    *amplitude = convert_to_kev(amplitude, *channel, &params.algorithm);
                }
            });
        }
        (time, events)
    }).collect::<BTreeMap<_, _>>()
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
    }
}


/// Extract events from single waveform.
/// Do not use this function directly without reason, use [extract_events](crate::process::extract_events) instead.
/// TODO: add ui argument description
pub fn frame_to_events(
    frame: &NumassFrame, 
    algorithm: &Algorithm,
    static_params: &StaticProcessParams,
    #[cfg(feature = "egui")] ui: Option<&mut PlotUi>
) -> Vec<NumassEvent> {
    
    let mut events = match algorithm {
        Algorithm::Max => {
            frame.iter().map(|(ch_id, waveform)| {
                let (x, y) = waveform
                    .iter()
                    .enumerate()
                    .max_by(|first, second| {
                        first.1.partial_cmp(second.1).unwrap()
                    })
                .unwrap();

                (x as u16 * 8, FrameEvent::Event { channel: *ch_id, amplitude: *y as f32, size: 1 })
            }).collect::<Vec<_>>()
        }
        Algorithm::Likhovid { left, right } => {
            frame.iter().map(|(ch_id, waveform)| {
                let (x, _) = waveform
                .iter()
                .enumerate()
                .max_by(|first, second| {
                    first.1.partial_cmp(second.1).unwrap()
                })
                .unwrap();

                let amplitude = {
                    let left = if x >= *left { x - left } else { 0 };
                    let right = std::cmp::min(waveform.len(), x + right);
                    let crop = &waveform[left..right];
                    crop.iter().sum::<i16>() as f32 / crop.len() as f32
                };

                (x as u16 * 8, FrameEvent::Event { channel: *ch_id, amplitude, size: 1 })
            }).collect::<Vec<_>>()
        }
        Algorithm::FirstPeak { threshold, left } => {
            frame.iter().filter_map(|(ch_id, waveform)| {
                find_first_peak(waveform, *threshold).map(|pos| {
                    let left = if pos < *left {
                        0
                    } else {
                        pos - left
                    };
                    // let length = (waveform.0.len() - pos) as f32;
                    let amplitude = waveform[left..waveform.len()].iter().sum::<i16>();
                    (pos as u16 * 8, FrameEvent::Event { channel: *ch_id, amplitude: amplitude as f32 / 50.0, size: 1 } )
                })
            }).collect::<Vec<_>>()
        }
        Algorithm::Trapezoid { 
            left, center, right, 
            treshold,
            min_length, 
            reset_detection: HWResetParams { 
                window: r_window, treshold: r_treshold, size: r_size } } => {

            let mut reset: Option<(usize, usize)> = None;
            frame.iter().for_each(|(_, waveform)| {
                for i in 0..(waveform.len() - r_window) {
                    if waveform[i] - waveform[i + r_window] > *r_treshold {
                        if let Some(reset_last) = reset {
                            reset = Some((i.min(reset_last.0), (i + r_size).max(reset_last.1)));
                        } else {
                            reset = Some((i, i + r_size))
                        }
                        break;
                    }
                }
            });

            #[cfg(feature = "egui")]
            let mut filtered_all = BTreeMap::new();

            let mut events = frame.iter().flat_map(|(ch_id, waveform)| {

                let baseline = if let StaticProcessParams { baseline: Some(baseline) } = static_params {
                    baseline[*ch_id as usize]
                } else {
                    0.0
                };
    
                let mut events = vec![];
    
                let offset = left + center + right;
    
                let filtered = waveform.windows(left + center + right).map(|window| {
                    (
                        window[left+center..].iter().map(|v| *v as i32).sum::<i32>() - 
                        window[..*left].iter().map(|v| *v as i32).sum::<i32>()
                    ) as f32 / (left + right) as f32 - baseline
                }).collect::<Vec<_>>();

                #[cfg(feature = "egui")]
                if ui.is_some() {
                    filtered_all.insert(*ch_id, filtered.clone());
                }
    
                let mut i = 0;
                while i < filtered.len() {
                    
                    if let Some((reset_start, reset_end)) = reset {
                        if i == reset_start - offset {
                            i = reset_end - offset;
                            continue;
                        }
                    }
                    
                    if (i == 0 ||  filtered[i - 1] < *treshold as f32) && filtered[i] >= *treshold as f32 {
                        let mut energy = 0.0;
                        let mut event_end = i;
    
                        while event_end < filtered.len() && filtered[event_end] >= *treshold as f32   {
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
                                FrameEvent::Event { channel: *ch_id, amplitude: energy / offset as f32, size: (event_end - i) as u16 }
                            ));
                        }
    
                        i = event_end;
                        continue;
                    }
    
                    i += 1;
                }

                events
            }).collect::<Vec<_>>();

            if let Some((reset_start, reset_end)) = reset {
                events.push((reset_start as u16 * 8, FrameEvent::Reset { size: (reset_end - reset_start) as u16 }));
            }

            #[cfg(feature = "egui")]
            if let Some(ui) = ui {

                for (ch_id, filtered) in filtered_all {
                    let offset = left + center + right;
                    let line = Line::new(
                        filtered.clone().into_iter().enumerate().map(|(idx, amp)| [(idx + offset) as f64, amp as f64]).collect::<Vec<_>>())
                        .color(color_for_index(ch_id as usize))
                        .name(format!("filtered + baseline ch# {}", ch_id + 1))
                        .style(egui_plot::LineStyle::Dashed { length: 10.0 });
                    ui.line(line);
                }

                ui.hline(
                    HLine::new(*treshold as f64)
                    .color(Color32::WHITE)
                    .name(format!("TRIGGER"))
                );

                events.iter().enumerate().for_each(|(idx, (pos, event))| {
                    match event {
                        &FrameEvent::Event { channel, amplitude, size } => {
                            let ch = channel + 1;
                            let name = format!("ev#{idx} ch# {ch}");

                            ui.vline(VLine::new((*pos as f64) / 8.0).color(color_for_index(channel as usize)).name(name.clone()));
                            ui.vline(VLine::new((*pos + size as u16 * 8) as f64 / 8.0).color(color_for_index(channel as usize)).name(name.clone()));
                            ui.points(Points::new(vec![[*pos as f64 / 8.0, amplitude as f64]])
                                .color(color_for_index(channel as usize))
                                .shape(MarkerShape::Diamond)
                                .filled(false)
                                .radius(10.0)
                                .name(name)
                            );
                        }
                        &FrameEvent::Reset { size } => {
                            ui.vline(VLine::new(*pos as f64 / 8.0).color(Color32::WHITE).name(format!("RESET")));
                            ui.vline(VLine::new((*pos + size as u16 * 8) as f64 / 8.0).color(Color32::WHITE).name(format!("RESET")));
                        }
                        _ => {
                            // TODO: implement
                        }
                    }
                
                })

            };

            events
        }
    };

    events.sort_by_key(|(pos, _)| *pos);

    events
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
