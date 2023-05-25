use std::{ops::Range, path::PathBuf, time::SystemTime};

use serde::{Deserialize, Serialize};

use processing::{histogram::PointHistogram, ProcessingParams, numass, Algorithm};

#[cfg(not(target_arch = "wasm32"))]
use {
    processing::{
        post_process, amplitudes_to_histogram, extract_amplitudes, numass::protos::rsb_event
    },
    protobuf::Message,
    std::path::Path,
};

pub const CACHE_DIRECTORY: &str = "CACHE_DIRECTORY";

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCache {
    pub opened: bool,
    pub meta: Option<numass::NumassMeta>,
    pub processed: Option<SystemTime>,
    pub histogram: Option<PointHistogram>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessRequest {
    CalcHist {
        filepath: PathBuf,
        processing: ProcessingParams,
    },
    // TODO: move to viewers crate
    FilterEvents {
        filepath: PathBuf,
        range: Range<f32>,
        neighborhood: usize,
        algorithm: Algorithm,
        convert_kev: bool
    },
    SplitTimeChunks {
        filepath: PathBuf,
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
}

// TODO: implement better caching algorithm
#[cfg(not(target_arch = "wasm32"))]
fn get_cache_key(root: &Path, filepath: &Path, params: &ProcessingParams) -> PathBuf {
    let raw_key = format!(
        "process_file-{}-{}-{}",
        filepath.as_os_str().to_str().unwrap(),
        serde_json::to_string(&params.algorithm).unwrap(),
        params.post_processing.convert_to_kev
    );
    let digest = md5::compute(raw_key);
    let key = hex::encode(digest.as_slice());

    let mut out = PathBuf::from(root);
    out.push(key);
    out
}

#[cfg(not(target_arch = "wasm32"))]
pub fn process_file(filepath: PathBuf, params: ProcessingParams) -> Option<FileCache> {
    use std::collections::BTreeMap;

    use dataforge::{read_df_header_and_meta_sync, read_df_message_sync, DFMessage};

    let processed = std::fs::metadata(&filepath).unwrap().modified().unwrap();

    let cache_key = std::env::var(CACHE_DIRECTORY).ok().map(|s| {
        let root = PathBuf::from(s);
        get_cache_key(&root, &filepath, &params)
    });

    let cached = cache_key
        .clone()
        .filter(|cache_key| cache_key.exists())
        .map(|cache_key| {
            let data = std::fs::read(cache_key).unwrap();
            rmp_serde::from_slice::<Option<BTreeMap<u64, BTreeMap<usize, f32>>>>(&data).unwrap()
        });

    let amplitudes = cached.unwrap_or_else(|| {
        let mut point_file = std::fs::File::open(&filepath).unwrap();
        if let Ok(DFMessage {
            meta: numass::NumassMeta::Reply(numass::Reply::AcquirePoint { .. }),
            data,
        }) = read_df_message_sync::<numass::NumassMeta>(&mut point_file)
        {
            let point = rsb_event::Point::parse_from_bytes(&data.unwrap()[..]).unwrap(); // return None for bad parsing
            let out = Some(extract_amplitudes(
                &point,
                &params.algorithm,
                params.post_processing.convert_to_kev,
            ));

            if let Some(cache_key) = &cache_key {
                std::fs::write(cache_key, rmp_serde::to_vec(&out).unwrap()).unwrap()
            }
            out
        } else {
            None
        }
    });

    amplitudes.map(|amps| {
        let mut point_file = std::fs::File::open(&filepath).unwrap();
        let (_, meta) =
            read_df_header_and_meta_sync::<numass::NumassMeta>(&mut point_file).unwrap();

        FileCache {
            opened: true,
            histogram: Some(
                amplitudes_to_histogram(post_process(amps, &params.post_processing), params.histogram)),
            processed: Some(processed),
            meta: Some(meta),
        }
    })
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
                expand_dir(entry.path())
            })
            .collect::<Vec<_>>();

        children.sort_by(|a, b| natord::compare(a.to_filename(), b.to_filename()));

        Some(FSRepr::Directory { path, children })
    } else {
        panic!()
    }
}
