//! # Viewer
//! Temporary module for viewer state and mode.
//! TODO: remove from numass-processing module
//! 
use std::{ops::Range, path::PathBuf, time::SystemTime};
use chrono::NaiveDateTime;
use serde::{Serialize, Deserialize};
use crate::{process::ProcessParams, postprocess::PostProcessParams, histogram::{PointHistogram, HistogramParams}};

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
                algorithm: crate::process::Algorithm::Trapezoid { left: 6, center: 15, right: 6 },
                convert_to_kev: true,
            },
            post_process: PostProcessParams {
                merge_close_events: false,
                ..Default::default()
            },
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
        neighborhood: usize,
        processing: ProcessParams,
    },
    Waveforms {
        filepath: PathBuf,
    },
    Bundles {
        filepath: PathBuf,
    },
}

// TODO: remove
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointState {
    pub opened: bool,
    pub modified: Option<SystemTime>,
    pub histogram: Option<PointHistogram>,
    pub voltage: Option<f32>,
    pub start_time: Option<NaiveDateTime>,
    pub counts: Option<usize>,
}