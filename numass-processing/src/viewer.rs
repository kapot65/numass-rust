//! # Viewer
//! Temporary module for viewer state and mode.
//! TODO: remove from numass-processing module
//!
use crate::{
    histogram::{HistogramParams, PointHistogram},
    postprocess::PostProcessParams,
    preprocess::{Preprocess, PreprocessParams},
    process::ProcessParams,
};
use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    ops::Range,
    path::PathBuf,
    time::SystemTime,
};

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ViewerState {
    #[serde(default)]
    pub preprocess: PreprocessParams,
    pub process: ProcessParams,
    pub post_process: PostProcessParams,
    pub histogram: HistogramParams,
    pub changed: bool,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            preprocess: PreprocessParams::default(),
            process: ProcessParams::default(),
            post_process: PostProcessParams::default(),
            histogram: HistogramParams {
                range: -50.0..100.0,
                bins: 300,
            },
            changed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToROOTOptions {
    pub filepath: PathBuf,
    pub preprocess: PreprocessParams,
    pub process: ProcessParams,
    pub postprocess: PostProcessParams,
}

impl Hash for ToROOTOptions {
    fn hash<H: Hasher>(&self, state: &mut H) {
        serde_json::to_string(self).unwrap().hash(state);
    }
}

impl ToROOTOptions {
    /// Query string для `GET /api/to-root`: `params=<json>`.
    pub fn to_query_string(&self) -> String {
        crate::query::to_query_string(self)
    }

    pub fn from_query_string(query: &str) -> Result<Self, String> {
        crate::query::from_query_string(query)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewerMode {
    FilteredEvents {
        filepath: PathBuf,
        range: Range<f32>,
        #[serde(default)]
        preprocess: PreprocessParams,
        process: ProcessParams,
        postprocess: PostProcessParams,
    },
    Waveforms {
        filepath: PathBuf,
    },
    Bundles {
        filepath: PathBuf,
        #[serde(default)]
        preprocess: PreprocessParams,
        process: ProcessParams,
        postprocess: PostProcessParams,
    },
    Triggers {
        filepath: PathBuf,
    },
}

impl ViewerMode {
    /// Query string окна вьювера: `params=<json>`, тот же вид, что у GET `/api`.
    pub fn to_query_string(&self) -> String {
        crate::query::to_query_string(self)
    }

    pub fn from_query_string(query: &str) -> Result<Self, String> {
        crate::query::from_query_string(query)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointState {
    pub opened: bool,
    pub modified: Option<SystemTime>,
    pub histogram: Option<PointHistogram>,
    pub preprocess: Option<Preprocess>,
    pub counts: Option<usize>,
}

pub const EMPTY_POINT: PointState = PointState {
    opened: false,
    histogram: None,
    preprocess: None,
    counts: None,
    modified: None,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postprocess::PostProcessParams;
    use crate::process::ProcessParams;

    #[test]
    fn to_root_options_roundtrip_query() {
        let options = ToROOTOptions {
            filepath: PathBuf::from("p4(30s)"),
            preprocess: PreprocessParams::default(),
            process: ProcessParams::default(),
            postprocess: PostProcessParams::default(),
        };
        let decoded = ToROOTOptions::from_query_string(&options.to_query_string()).unwrap();
        assert_eq!(
            serde_json::to_string(&options).unwrap(),
            serde_json::to_string(&decoded).unwrap()
        );
    }

    #[test]
    fn viewer_mode_roundtrip_query() {
        let modes = [
            ViewerMode::FilteredEvents {
                filepath: PathBuf::from("p4(30s)(HV1=18300)"),
                range: -50.0..100.0,
                preprocess: PreprocessParams::default(),
                process: ProcessParams::default(),
                postprocess: PostProcessParams::default(),
            },
            ViewerMode::Waveforms {
                filepath: PathBuf::from("p4(30s)(HV1=18300)"),
            },
            ViewerMode::Bundles {
                filepath: PathBuf::from("p4(30s)(HV1=18300)"),
                preprocess: PreprocessParams::default(),
                process: ProcessParams::default(),
                postprocess: PostProcessParams::default(),
            },
            ViewerMode::Triggers {
                filepath: PathBuf::from("p4(30s)(HV1=18300)"),
            },
        ];

        for mode in modes {
            let query = mode.to_query_string();
            assert!(query.starts_with("params="), "{query}");
            let decoded = ViewerMode::from_query_string(&query).unwrap();
            assert_eq!(
                serde_json::to_string(&mode).unwrap(),
                serde_json::to_string(&decoded).unwrap()
            );
        }
    }
}
