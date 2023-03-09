use std::{collections::BTreeMap, ops::Range, path::PathBuf, time::SystemTime};

#[cfg(not(target_arch = "wasm32"))]
use {
    dataforge::read_df_message,
    processing::{
        amplitudes_to_histogramm, convert_to_kev, extract_amplitudes, frame_to_waveform,
        waveform_to_event,
    },
    protobuf::Message,
    std::path::Path,
};

use numass::protos::rsb_event;
use processing::{histogram::PointHistogram, ProcessingParams};
use serde::{Deserialize, Serialize};

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
        params: ProcessingParams,
    },
    FilterEvents {
        filepath: PathBuf,
        range: Range<f32>,
        neigborhood: u64,
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

#[cfg(not(target_arch = "wasm32"))]
fn get_cache_key(root: &Path, filepath: &Path, params: &ProcessingParams) -> PathBuf {
    let raw_key = format!(
        "process_file-{}-{}-{}",
        filepath.as_os_str().to_str().unwrap(),
        serde_json::to_string(&params.algorithm).unwrap(),
        params.convert_to_kev
    );
    let digest = md5::compute(raw_key);
    let key = hex::encode(digest.as_slice());

    let mut out = PathBuf::from(root);
    out.push(key);
    out
}

#[cfg(not(target_arch = "wasm32"))]
pub fn process_file(filepath: PathBuf, params: ProcessingParams) -> Option<FileCache> {
    use dataforge::{read_df_header_and_meta_sync, read_df_message_sync, DFMessage};

    use crate::CACHE_DIRECTORY;

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
                params.convert_to_kev,
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
        // TODO: make all in a single file read
        let mut point_file = std::fs::File::open(&filepath).unwrap();
        let (_, meta) =
            read_df_header_and_meta_sync::<numass::NumassMeta>(&mut point_file).unwrap();

        FileCache {
            opened: true,
            histogram: Some(amplitudes_to_histogramm(amps, params)),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFrame {
    pub time: u64,
    pub waveforms: BTreeMap<u8, Vec<i16>>,
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn filter_events(
    path: &PathBuf,
    range: &Range<f32>,
    neigborhood: u64,
) -> Vec<(DeviceFrame, Vec<DeviceFrame>)> {
    let mut point_file = tokio::fs::File::open(&path).await.unwrap();
    let message = read_df_message::<numass::NumassMeta>(&mut point_file)
        .await
        .unwrap();

    let mut events: BTreeMap<u64, BTreeMap<u8, Vec<i16>>> = BTreeMap::new();

    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
    for channel in &point.channels {
        for block in &channel.blocks {
            for frame in &block.frames {
                let entry = events.entry(frame.time).or_default();
                entry.insert(channel.id as u8, frame_to_waveform(frame));
            }
        }
    }

    let algorithm = processing::Algorithm::Likhovid { left: 6, right: 36 };

    let events = events
        .iter()
        .map(|(time, waveforms)| (*time, waveforms.clone()))
        .collect::<Vec<_>>();

    events
        .iter()
        .enumerate()
        .filter_map(|(idx, (time, waveforms))| {
            // if waveforms.len() != 1 || !waveforms.contains_key(&5) {
            //     return None;
            // }

            if !waveforms
                .iter()
                .map(|(ch_id, waveform)| {
                    let (_, amp) = waveform_to_event(waveform, &algorithm);
                    convert_to_kev(&amp, *ch_id, &algorithm)
                })
                .any(|amp| range.contains(&amp))
            {
                return None;
            }

            let neighbors = {
                let bounds = (*time - neigborhood)..(*time + neigborhood);
                let mut neighbors = vec![];
                let mut left = idx;
                loop {
                    if left == 0 {
                        break;
                    }
                    if !bounds.contains(&events[left - 1].0) {
                        break;
                    }
                    neighbors.push(DeviceFrame {
                        time: events[left - 1].0,
                        waveforms: events[left - 1].1.clone(),
                    });
                    left -= 1;
                }
                let mut right = idx;
                loop {
                    if right == events.len() - 1 {
                        break;
                    }
                    if !bounds.contains(&events[right + 1].0) {
                        break;
                    }
                    neighbors.push(DeviceFrame {
                        time: events[right + 1].0,
                        waveforms: events[right + 1].1.clone(),
                    });
                    right += 1;
                }
                neighbors
            };
            Some((
                DeviceFrame {
                    time: *time,
                    waveforms: waveforms.clone(),
                },
                neighbors,
            ))
        })
        .collect::<Vec<_>>()
}

pub fn point_to_chunks(point: rsb_event::Point) -> Vec<Vec<(u8, Vec<[f64; 2]>)>> {
    let limit_ns = 1_000_000;

    let mut chunks = vec![];
    chunks.push(vec![]);

    for channel in point.channels {
        for block in channel.blocks {
            for frame in block.frames {
                let chunk_num = (frame.time / limit_ns) as usize;

                while chunks.len() < chunk_num + 1 {
                    chunks.push(vec![])
                }

                // TODO: Refactor with processing::frame_to_waveform
                let waveform_len = frame.data.len() / 2;
                let waveform = (0..waveform_len).map(|idx| {
                    let x =
                        (frame.time + 8u64 * (idx as u64) - (chunk_num as u64 * limit_ns)) as f64;
                    let y = i16::from_le_bytes(frame.data[idx * 2..idx * 2 + 2].try_into().unwrap())
                        as f64;
                    [x / 1000.0, y]
                });

                let baseline = waveform.clone().take(16).map(|[_, y]| y).sum::<f64>() / 16.0;
                chunks[chunk_num].push((
                    channel.id as u8,
                    waveform.map(|[x, y]| [x, y - baseline]).collect::<Vec<_>>(),
                ))
            }
        }
    }

    chunks
}
