//! # Storage
//! High-level processing + storage functions
//! Should work with both local and remote storage.
//! If possible, use functions from this module instead of [process](crate::process) and [postprocess](crate::postprocess) directly.
use std::{fs, path::{Path, PathBuf}, time::SystemTime};

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
                    point,
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
        if let Ok(mut point_file) = tokio::fs::File::open(&filepath).await {
            let message = dataforge::read_df_message::<numass::NumassMeta>(&mut point_file)
            .await
            .unwrap();
            rsb_event::Point::parse_from_bytes(&message.data.unwrap_or(vec![])[..]).unwrap()
        } else {
            panic!("{filepath:?} open failed")
        }
    }
}

/// Temporal numass file storage representation.
/// TODO: switch to real numass storage service when it will be implemented.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LoadState {
    NotLoaded,
    NeedLoad,
    Loaded
}

impl Default for LoadState {
    fn default() -> Self {
        Self::NotLoaded
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FSRepr {
    File {
        path: PathBuf,
        modified: SystemTime
    },
    Directory {
        path: PathBuf,
        children: Vec<FSRepr>,
        modified: SystemTime,
        #[serde(skip_serializing, default)]
        load_state: LoadState
    },
}

impl FSRepr {
    pub fn to_filename(&self) -> PathBuf {
        let path = match self {
            FSRepr::File { path, .. } => path,
            FSRepr::Directory { path, .. } => path,
        };
        path.to_owned()
    }

    pub fn new(path: PathBuf) -> FSRepr {
        FSRepr::Directory { 
            path, 
            children: vec![], 
            load_state: LoadState::NotLoaded, 
            modified: SystemTime::UNIX_EPOCH
        }
    }

    pub async fn reload(self) {
        unimplemented!()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn ls(path: PathBuf) -> FSRepr {
        let meta = tokio::fs::metadata(&path).await.unwrap();

        if meta.is_file() {
            return FSRepr::File { path, modified: meta.modified().unwrap() }
        }

        if meta.is_dir() {
            let mut read_dir = tokio::fs::read_dir(&path).await.unwrap();

            let mut children = vec![];
            while let Ok(Some(child)) = read_dir.next_entry().await {
                let meta = child.metadata().await.unwrap();
                let path = child.path();
                if meta.is_file() {
                    children.push(FSRepr::File { path, modified: meta.modified().unwrap() });
                } else if meta.is_dir() {
                    children.push(FSRepr::Directory { 
                        path, children: vec![], 
                        modified: meta.modified().unwrap(), 
                        load_state: LoadState::NotLoaded 
                    });
                }
            }
            FSRepr::Directory { 
                path, children, 
                modified: meta.modified().unwrap(), 
                load_state: LoadState::NotLoaded 
            }
        } else {
            panic!("neither file, nor directory!")
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn ls(path: PathBuf) -> FSRepr {
        let payload = gloo::net::http::Request::get(&api_url("api/ls", &path))
        .send()
        .await
        .unwrap()
        .binary()
        .await
        .unwrap();
        serde_json::from_slice(&payload).unwrap() // TODO: change to Request (to remove serde-json)?
    }

    pub async fn expand(path: PathBuf, children: &mut Vec<FSRepr>, modified: &mut SystemTime, load_state: &mut LoadState) {
        *load_state = LoadState::Loaded;
        let updated = FSRepr::ls(path.to_owned()).await;
        if let FSRepr::Directory { 
            children: children_upd, 
            modified: modified_upd, 
            ..
        } = updated {
            let mut to_merge = children_upd.into_iter().map(|child| (child.to_filename(), child)).collect::<std::collections::HashMap<_,_>>();
            for child in children.iter() {
                if let Some(place) = to_merge.get_mut(&child.to_filename()) {
                    *place = child.clone() // TODO: update modified time in children
                }
            }
            *children = to_merge.into_values().collect::<Vec<_>>();
            children.sort_by(|v1, v2| natord::compare(
                v1.to_filename().as_os_str().to_str().unwrap(), 
                v2.to_filename().as_os_str().to_str().unwrap()));

            *modified = modified_upd;
        };
    }

    pub async fn expand_reccurently(&mut self) {
        let mut current = vec![self];
        loop {
            if current.is_empty() {
                break;
            }
            let mut next = vec![];
            for child in current {
                if let FSRepr::Directory { path, children, modified, load_state } = child {

                    match load_state {
                        LoadState::NeedLoad => {
                            FSRepr::expand(path.to_path_buf(), children, modified, load_state).await;
                            for ele in children.iter_mut() {
                                next.push(ele);
                            }
                        }
                        LoadState::Loaded => {
                            for ele in children.iter_mut() {
                                next.push(ele);
                            }
                        }
                        LoadState::NotLoaded => {}
                    }
                }
            }
            current = next;
        }
    }

    pub async fn update_reccurently(&mut self) {
        let mut current = vec![self];
        loop {
            if current.is_empty() {
                break;
            }
            let mut next = vec![];
            for leaf in current {
                if let FSRepr::Directory { path, children, modified, load_state } = leaf {
                    match load_state {
                        LoadState::Loaded => {
                            if let FSRepr::Directory { 
                                children: mut children_new, 
                                modified: modified_new, 
                                ..
                            } = FSRepr::ls(path.to_owned()).await {
                                children_new.iter_mut().for_each(|child_new| {
                                    if let Some(child) = children.iter().find(|child| child.to_filename() == child_new.to_filename()) {
                                        if let (
                                            FSRepr::Directory { load_state: LoadState::Loaded, .. }, 
                                            FSRepr::Directory { load_state, .. }
                                        ) = (child, child_new) {
                                                *load_state = LoadState::Loaded;
                                        }
                                    }
                                });
                                children_new.sort_by(|v1, v2| natord::compare(
                                    v1.to_filename().as_os_str().to_str().unwrap(), 
                                    v2.to_filename().as_os_str().to_str().unwrap()));
                                *children = children_new;
                                *modified = modified_new;
                            }

                            for ele in children.iter_mut() {
                                next.push(ele);
                            }
                        }
                        LoadState::NeedLoad => {
                        }
                        LoadState::NotLoaded => {}
                    }
                }
            }
            current = next;
        }
    }


}