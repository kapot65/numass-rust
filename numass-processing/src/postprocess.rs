//! # Postprocessing
//! This module contains built-in postrpocessing
//! (additional processing for events already extracted from waveforms)
//! see [params](crate::postprocess::PostProcessParams) for details.
//!

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    constants::DETECTOR_BORDERS,
    preprocess::{Preprocess, CUTOFF_BIN_SIZE},
    types::{FrameEvent, NumassEvent, NumassEvents},
};

#[cfg(feature = "egui")]
use {
    crate::utils::color_for_index,
    egui_plot::{Line, LineStyle, MarkerShape, PlotUi, Points},
    std::collections::HashSet,
};

#[repr(C)]
/// Postprocessing params.
#[derive(PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct PostProcessParams {
    /// remove events inside [bad_blocks](crate::preprocess::PreprocessParams::bad_blocks) timestamps
    pub cut_bad_blocks: bool,

    pub merge_splits_first: bool,
    pub merge_close_events: bool,
    pub ignore_borders: bool,

    /// ignore channels with index in this array set to true. Default is false for all channels.
    pub ignore_channels: [bool; 7]
}

impl Default for PostProcessParams {
    fn default() -> Self {
        Self {
            cut_bad_blocks: true,
            merge_splits_first: false,
            merge_close_events: true,
            ignore_borders: false,
            ignore_channels: [false; 7]
        }
    }
}

fn ignore_channels(ignore_channels: &[bool; 7], amplitudes: &mut NumassEvents) {
    if *ignore_channels != [false; 7] {
        amplitudes.iter_mut().for_each(|(_, events)| {
            events.retain(|(_, event)| match event {
                FrameEvent::Event { channel, .. } => !ignore_channels[*channel as usize],
                _ => true,
            });
        });
    }
}

/// Built-in postprocessing algorithm.
/// > [!NOTE]  
/// > For now cut_bad_blocks works only with `merge_close_events` set to true.
/// > TODO: fix it.
pub fn post_process(
    process_result: (NumassEvents, Preprocess),
    params: &PostProcessParams,
) -> (NumassEvents, Preprocess) {
    let (amplitudes, preprocess_params) = process_result;

    if !params.merge_close_events {
        let mut amplitudes = amplitudes;
        ignore_channels(&params.ignore_channels, &mut amplitudes);
        return (amplitudes, preprocess_params);
    }

    // TODO: think about code deduplication
    let mut amplitudes = if params.cut_bad_blocks {
        amplitudes
            .into_iter()
            .filter(|(timestamp, _)| {
                let curr_block = (timestamp / CUTOFF_BIN_SIZE) as usize;
                !preprocess_params.bad_blocks.contains(&curr_block)
            })
            .map(|(time, events)| {
                let events_postprocessed = post_process_frame(
                    events,
                    params,
                    #[cfg(feature = "egui")]
                    None,
                );
                (time, events_postprocessed)
            })
            .collect::<BTreeMap<_, _>>()
    } else {
        amplitudes
            .into_iter()
            .map(|(time, events)| {
                let events_postprocessed = post_process_frame(
                    events,
                    params,
                    #[cfg(feature = "egui")]
                    None,
                );
                (time, events_postprocessed)
            })
            .collect::<BTreeMap<_, _>>()
    };

    ignore_channels(&params.ignore_channels, &mut amplitudes);

    (amplitudes, preprocess_params)
}

fn is_neighbour(ch_1: u8, ch_2: u8) -> bool {
    if ch_1 == ch_2 {
        return true;
    }

    if ch_1 == 5 || ch_2 == 5 {
        return true;
    }

    let border = if ch_1 < ch_2 {
        [ch_1, ch_2]
    } else {
        [ch_2, ch_1]
    };

    DETECTOR_BORDERS.contains(&border)
}

fn merge_splits(
    mut events: Vec<NumassEvent>,
    #[cfg(feature = "egui")] ui: &mut Option<&mut PlotUi>,
) -> Vec<NumassEvent> {
    #[cfg(feature = "egui")]
    let mut merges = vec![];

    let mut to_remove = vec![];

    let mut idx = 0;
    while idx < events.len() {
        if to_remove.contains(&idx) {
            idx += 1;
            continue;
        }

        if let (
            offset,
            FrameEvent::Event {
                channel,
                mut amplitude,
                size: _,
            },
        ) = events[idx]
        {
            if channel == 5 {
                let mut idx_past = (idx - 1) as isize;
                while idx_past >= 0 && events[idx_past as usize].0.abs_diff(offset) < 200 {
                    if let (
                        _offset_past,
                        FrameEvent::Event {
                            #[cfg(feature = "egui")]
                                channel: channel_past,
                            amplitude: amplitude_past,
                            ..
                        },
                    ) = events[idx_past as usize]
                    {
                        amplitude += amplitude_past;
                        to_remove.push(idx_past as usize);

                        #[cfg(feature = "egui")]
                        merges.push((idx, (channel_past, _offset_past, amplitude_past)));
                    }
                    idx_past -= 1;
                }

                if idx != events.len() - 1 {
                    let mut idx_next = idx + 1;
                    while idx_next < events.len() && events[idx_next].0.abs_diff(offset) < 200 {
                        if let (
                            _offset_next,
                            FrameEvent::Event {
                                #[cfg(feature = "egui")]
                                    channel: channel_next,
                                amplitude: amplitude_next,
                                ..
                            },
                        ) = events[idx_next]
                        {
                            amplitude += amplitude_next;
                            to_remove.push(idx_next);

                            #[cfg(feature = "egui")]
                            merges.push((idx, (channel_next, _offset_next, amplitude_next)));
                        }
                        idx_next += 1;
                    }
                }
            }

            events[idx] = (
                offset,
                FrameEvent::Event {
                    channel,
                    amplitude,
                    size: 0,
                },
            );
        }

        idx += 1;
    }

    #[cfg(feature = "egui")]
    if let Some(ui) = ui {
        for (idx, (channel2, pos2, amplitude2)) in merges {
            if let (pos1, FrameEvent::Event { amplitude, .. }) = events[idx] {
                ui.line(
                    Line::new(vec![
                        [pos1 as f64 / 8.0, amplitude as f64],
                        [pos2 as f64 / 8.0, amplitude2 as f64],
                    ])
                    .color(color_for_index(channel2 as usize))
                    .style(LineStyle::dotted_loose()),
                );
            }
        }
    }

    to_remove.sort();
    to_remove.iter().rev().for_each(|&idx| {
        if idx < events.len() {
            // TODO: find solution
            events.remove(idx);
        }
    });

    events
}

pub fn post_process_frame(
    mut events: Vec<NumassEvent>,
    params: &PostProcessParams,
    #[cfg(feature = "egui")] mut ui: Option<&mut PlotUi>,
) -> Vec<NumassEvent> {
    if !params.merge_close_events {
        return events;
    }

    if params.merge_splits_first {
        events = merge_splits(
            events,
            #[cfg(feature = "egui")]
            &mut ui,
        );
    }

    #[cfg(feature = "egui")]
    let mut merges = vec![];

    let mut idx = 0;
    while idx < events.len() {
        if let (
            offset,
            FrameEvent::Event {
                channel,
                mut amplitude,
                size,
            },
        ) = events[idx]
        {
            let mut idx_next = events.len() - 1;
            while idx_next > idx {
                if let (
                    _pos2,
                    FrameEvent::Event {
                        channel: channel_next,
                        amplitude: amplitude_next,
                        ..
                    },
                ) = events[idx_next]
                {
                    if params.ignore_borders || is_neighbour(channel, channel_next) {
                        #[cfg(feature = "egui")]
                        merges.push((idx, (channel_next, _pos2, amplitude_next)));

                        amplitude += amplitude_next;
                        events.remove(idx_next);
                    }
                }
                idx_next -= 1;
            }
            events[idx] = (
                offset,
                FrameEvent::Event {
                    channel,
                    amplitude,
                    size,
                },
            );
        }
        idx += 1;
    }

    #[cfg(feature = "egui")]
    if let Some(ui) = ui {
        // draw merged points first
        let merged_idxs = merges.iter().map(|(idx, _)| *idx).collect::<HashSet<_>>();
        for idx in merged_idxs {
            if let (
                pos,
                FrameEvent::Event {
                    channel, amplitude, ..
                },
            ) = events[idx]
            {
                let name = format!("ch# {channel} merged");
                ui.points(
                    Points::new(vec![[pos as f64 / 8.0, amplitude as f64]])
                        .color(color_for_index(channel as usize))
                        .shape(MarkerShape::Circle)
                        .filled(false)
                        .radius(10.0)
                        .name(name),
                );
            }
        }

        for (idx, (channel2, pos2, amplitude2)) in merges {
            if let (pos1, FrameEvent::Event { amplitude, .. }) = events[idx] {
                ui.line(
                    Line::new(vec![
                        [pos1 as f64 / 8.0, amplitude as f64],
                        [pos2 as f64 / 8.0, amplitude2 as f64],
                    ])
                    .color(color_for_index(channel2 as usize))
                    .style(LineStyle::dotted_loose()),
                );
            }
        }
    }

    events
}
