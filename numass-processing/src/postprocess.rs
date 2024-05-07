//! # Postprocessing
//! This module contains built-in postrpocessing
//! (additional processing for events already extracted from waveforms)
//! see [params](crate::postprocess::PostProcessParams) for details.
//! 
// use std::collections::BTreeMap;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{constants::DETECTOR_BORDERS, types::{FrameEvent, NumassEvents}};

/// Postprocessing params.
#[derive(PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct PostProcessParams {
    pub merge_close_events: bool,
}

impl Default for PostProcessParams {
    fn default() -> Self {
        Self {
            merge_close_events: true,
        }
    }
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

/// Built-in postprocessing algorithm.
pub fn post_process(amplitudes: NumassEvents, params: &PostProcessParams) -> NumassEvents {

    if !params.merge_close_events {
        return amplitudes;
    }

    amplitudes.into_iter().map(|(time, frames)| {

        let mut frames = frames;

        let mut idx = 0;
        while idx < frames.len() {

            if let (offset, FrameEvent::Event { channel, mut amplitude, size }) = frames[idx] {
                let mut idx_next = frames.len() - 1;
                while idx_next > idx {
                    if let FrameEvent::Event { channel: channel_next, amplitude: amplitude_next, .. } = frames[idx_next].1 {
                        if is_neighbour(channel, channel_next) {
                            amplitude += amplitude_next;
                            frames.remove(idx_next);
                        }
                    }
                    idx_next -= 1;
                }
                frames[idx] = (offset, FrameEvent::Event { channel, amplitude, size });
            }
            idx += 1;
        }

        (time, frames)
    }).collect::<BTreeMap<_,_>>()
    
}