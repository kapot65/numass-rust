//! # Viewer
//! Temporary module for viewer state and mode.
//! TODO: remove from numass-processing module
//! 
use std::{collections::BTreeSet, ops::Range, path::PathBuf, time::SystemTime};
use chrono::NaiveDateTime;
use serde::{Serialize, Deserialize};
use crate::{histogram::{HistogramParams, PointHistogram}, postprocess::PostProcessParams, process::{ProcessParams, TRAPEZOID_DEFAULT}};

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ViewerState {
    pub process: ProcessParams,
    pub post_process: PostProcessParams,
    pub histogram: HistogramParams,
    pub changed: bool
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            process: ProcessParams {
                algorithm: TRAPEZOID_DEFAULT,
                convert_to_kev: true,
            },
            post_process: PostProcessParams::default(),
            histogram: HistogramParams { range: 0.0..40.0, bins: 400 },
            changed: false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewerMode {
    FilteredEvents {
        filepath: PathBuf,
        range: Range<f32>,
        process: ProcessParams,
        postprocess: PostProcessParams,
    },
    Waveforms {
        filepath: PathBuf,
    },
    Bundles {
        filepath: PathBuf,
        process: ProcessParams,
        postprocess: PostProcessParams,
    },
    Triggers {
        filepath: PathBuf,
    }
}

// TODO: remove
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointState {
    pub opened: bool,
    pub modified: Option<SystemTime>,
    pub histogram: Option<PointHistogram>,
    pub voltage: Option<f32>,
    pub start_time: Option<NaiveDateTime>,
    pub acquisition_time: Option<f32>,
    pub bad_blocks: Option<BTreeSet<usize>>,
    pub counts: Option<usize>,
}

pub const EMPTY_POINT: PointState = PointState {
    opened: false,
    histogram: None,
    voltage: None,
    start_time: None,
    acquisition_time: None,
    bad_blocks: None,
    counts: None,
    modified: None,
};