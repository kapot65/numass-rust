//! # Postprocessing
//! This module contains built-in postrpocessing
//! (additional processing for events already extracted from waveforms)
//! see [params](crate::postprocess::PostProcessParams) for details.
//! 
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::NumassEvents;

/// Postprocessing params.
#[derive(PartialEq, Clone, Copy, Debug, Serialize, Deserialize, Hash)]
pub struct PostProcessParams {
    pub merge_close_events: bool,
    pub merge_map: [[bool; 7]; 7], // merge with merge_close_events via Option
    pub use_dead_time: bool,
    pub effective_dead_time: u64,
}

impl Default for PostProcessParams {
    fn default() -> Self {
        Self {
            merge_close_events: true,
            use_dead_time: false,
            effective_dead_time: 4000,
            merge_map: [
                [false, true, false, false, false, false, false],
                [false, false, false, true, false, false, false],
                [false, false, false, false, true, false, false],
                [false, false, false, false, false, false, true],
                [true, false, false, false, false, false, false],
                [true, true, true, true, true, false, true],
                [false, false, true, false, false, false, false],
            ],
        }
    }
}

/// Built-in postprocessing algorithm.
pub fn post_process(mut amplitudes: NumassEvents, params: &PostProcessParams) -> NumassEvents {

    let mut last_time: u64 = 0;
    amplitudes.iter_mut().filter_map(|(time, channels)| {

        if params.use_dead_time && last_time.abs_diff(*time) < params.effective_dead_time {
            return None;
        }

        last_time = *time;

        // TODO: add time analysis
        if params.merge_close_events {
            for ch_1 in 0..7 {
                for ch_2 in 0..7 {
                    if params.merge_map[ch_1][ch_2]
                        && channels.contains_key(&ch_1)
                        && channels.contains_key(&ch_2)
                    {
                        let amp2 = channels.get(&ch_2).unwrap().to_owned().1;
                        channels.entry(ch_1).and_modify(|(_, amp)| *amp += amp2);
                        channels.remove_entry(&ch_2).unwrap();
                    }
                }
            }
        }

        Some((*time, channels.clone()))
    }).collect::<BTreeMap<_,_>>()
    
}