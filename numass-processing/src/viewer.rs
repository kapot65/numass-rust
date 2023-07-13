use std::{path::PathBuf, ops::Range};
use serde::{Serialize, Deserialize};
use crate::{PostProcessParams, histogram::{PointHistogram, HistogramParams}, ProcessParams};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FSRepr {
    File {
        path: PathBuf,
    },
    Directory {
        path: PathBuf,
        children: Vec<FSRepr>,
    },
}

impl FSRepr {
    pub fn to_filename(&self) -> &str {
        let path = match self {
            FSRepr::File { path } => path,
            FSRepr::Directory { path, children: _ } => path,
        };
        path.file_name().unwrap().to_str().unwrap()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn expand_dir(path: PathBuf) -> Option<FSRepr> {

        let meta = std::fs::metadata(&path).unwrap();
        if meta.is_file() {
            Some(FSRepr::File { path })
        } else if meta.is_dir() {
            let children = std::fs::read_dir(&path).unwrap();

            let mut children = children
                .filter_map(|child| {
                    let entry = child.unwrap();
                    FSRepr::expand_dir(entry.path())
                })
                .collect::<Vec<_>>();

            children.sort_by(|a, b| natord::compare(a.to_filename(), b.to_filename()));

            Some(FSRepr::Directory { path, children })
        } else {
            panic!()
        }
    }
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ViewerState {
    pub process: ProcessParams,
    pub post_process: PostProcessParams,
    pub histogram: HistogramParams,
}

impl Default for ViewerState {
    fn default() -> Self {
        Self {
            process: ProcessParams::default(),
            post_process: PostProcessParams::default(),
            histogram: HistogramParams { range: 0.0..27.0, bins: 270 }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewerMode {
    FilterEvents {
        filepath: PathBuf,
        range: Range<f32>,
        neighborhood: usize,
        processing: ProcessParams,
    },
    SplitTimeChunks {
        filepath: PathBuf,
    },
}

// TODO: remove
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCache {
    pub opened: bool,
    pub meta: Option<numass::NumassMeta>,
    pub histogram: Option<PointHistogram>,
}