//! # Storage
//! High-level processing + storage functions
//! Should work with both local and remote storage.
//! If possible, use functions from this module instead of [process](crate::process) and [postprocess](crate::postprocess) directly.
use std::{fs::{self, metadata}, path::{Path, PathBuf}, time::SystemTime};

use numass::NumassMeta;
use protobuf::Message;
use serde::{Deserialize, Serialize};

use crate::{process::ProcessParams, types::NumassEvents, numass::protos::rsb_event};

/// Process point from the storage.
/// This function will load point from storage (both local and remote) and executes [extract_events](crate::process::extract_events).
pub async fn process_point(filepath: &Path, process: &ProcessParams) -> Option<(NumassMeta, Option<NumassEvents>)> {

    let meta = load_meta(filepath).await;

    if let Some(NumassMeta::Reply(numass::Reply::AcquirePoint { 
        ..
     })) = &meta {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let point = load_point(filepath).await;
            Some((
                meta.unwrap(),
                Some(crate::process::extract_events(
                    &point,
                    process,
                ))
            ))
        }

        #[cfg(target_arch = "wasm32")]
        {
            let amplitudes_raw = gloo::net::http::Request::post(&api_url("api/process", filepath))
                    .json(&process).unwrap()
                    .send()
                    .await
                    .unwrap()
                    .binary()
                    .await
                    .unwrap();
            Some((
                meta.unwrap(),
                rmp_serde::from_slice::<Option<NumassEvents>>(&amplitudes_raw).unwrap()
            ))
        }
    } else {
        None
    }
}


#[cfg(target_arch = "wasm32")]
/// Construct API url for the file.
/// This function is needed to work inside web worker.
pub fn api_url(prefix: &str, filepath: &Path) -> String {
    // TODO: change to gloo function when it comes out
    let base_url = js_sys::eval("String(new URL(self.location.href).origin)").unwrap().as_string().unwrap();
    format!("{base_url}/{prefix}{}", filepath.to_str().unwrap())
}

/// Load point metadata only from the storage.
pub async fn load_meta(filepath: &Path) -> Option<NumassMeta> {

    #[cfg(target_arch = "wasm32")]
    {
        gloo::net::http::Request::get(&api_url("api/meta", filepath))
            .send()
            .await
            .unwrap()
            .json::<Option<NumassMeta>>()
            .await
            .unwrap()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
        dataforge::read_df_header_and_meta::<numass::NumassMeta>(&mut point_file).await.map_or(
            None, |(_, meta)| Some(meta))
    }
}

pub async fn load_modified_time(filepath: PathBuf) -> Option<SystemTime> {
    if let Ok(metadata) = fs::metadata(filepath) {
        if let Ok(modified) = metadata.modified() {
            Some(modified)
        } else {
            None
        }
    } else {
        None
    }
}

/// Load and parse point binary data from the storage.
/// Do not use this function directly without reason.
pub async fn load_point(filepath: &Path) -> rsb_event::Point {
    #[cfg(target_arch = "wasm32")]
    {
        let point_data = gloo::net::http::Request::get(&api_url("files", filepath))
            .send()
            .await
            .unwrap()
            .binary()
            .await
            .unwrap();

        let mut buf = std::io::Cursor::new(point_data);
        let message: dataforge::DFMessage<NumassMeta> =
            dataforge::read_df_message_sync::<NumassMeta>(&mut buf).unwrap();

        rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap()
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
        let message = dataforge::read_df_message::<numass::NumassMeta>(&mut point_file)
            .await
            .unwrap();
        rsb_event::Point::parse_from_bytes(&message.data.unwrap_or(vec![])[..]).unwrap()
    }
}

/// Temporal numass file storage representation.
/// TODO: switch to real numass storage service when it will be implemented.
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