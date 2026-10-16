//! # Storage
//! High-level processing + storage functions
//! Should work with both local and remote storage.
//! If possible, use functions from this module instead of [process](crate::process) and [postprocess](crate::postprocess) directly.
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use numass::NumassMeta;
use protobuf::Message;
use serde::{Deserialize, Serialize};

use crate::{
    numass::protos::rsb_event,
    postprocess::PostProcessParams,
    preprocess::{Preprocess, PreprocessParams},
    process::ProcessParams,
    types::NumassEvents,
};

/// Process point from the storage.
/// This function will load point from storage (both local and remote) and executes [extract_events](crate::process::extract_events).
pub async fn process_point(
    filepath: &Path,
    preprocess: &PreprocessParams,
    process: &ProcessParams,
    postprocess: Option<&PostProcessParams>,
) -> Option<(NumassMeta, Option<(NumassEvents, Preprocess)>)> {
    let meta = load_meta(filepath).await;

    if let Some(NumassMeta::Reply(numass::Reply::AcquirePoint { .. })) = &meta {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // TODO: remove duplication with wasm32 branch. Maybe use async closures? https://rust-lang.github.io/async-book/07_workarounds/05_async_closures.html
            let point = load_point(filepath).await;

            let events = if let Some(postprocess) = postprocess {
                crate::postprocess::post_process(
                    crate::process::extract_events(meta.clone(), point, preprocess, process),
                    postprocess,
                )
            } else {
                crate::process::extract_events(meta.clone(), point, preprocess, process)
            };

            Some((meta.clone().unwrap(), Some(events)))
        }

        #[cfg(target_arch = "wasm32")]
        {
            let query = crate::process::HistParams {
                preprocess: preprocess.clone(),
                process: process.clone(),
                postprocess: postprocess.copied(),
            }
            .to_query_string();
            let amplitudes_raw = gloo::net::http::Request::get(&format!(
                "{}?{query}",
                api_url("api/process/hist", filepath)
            ))
            .send()
            .await
            .unwrap()
            .binary()
            .await
            .unwrap();
            Some((
                meta.unwrap(),
                rmp_serde::from_slice::<Option<(NumassEvents, Preprocess)>>(&amplitudes_raw)
                    .unwrap(),
            ))
        }
    } else {
        None
    }
}

#[cfg(target_arch = "wasm32")]
/// Construct API url for the path relative to the storage root.
/// The empty path addresses the storage root itself.
/// This function is needed to work inside web worker.
pub fn api_url(prefix: &str, filepath: &Path) -> String {
    // TODO: change to gloo function when it comes out
    let base_url = js_sys::eval("String(new URL(self.location.href).origin)")
        .unwrap()
        .as_string()
        .unwrap();
    let path = filepath.to_str().unwrap();
    if path.is_empty() {
        format!("{base_url}/{prefix}")
    } else {
        format!("{base_url}/{prefix}/{path}")
    }
}

/// DF01 header length. Known to the client; the file server does not parse it.
const DF_HEADER_LEN: usize = 30;

fn read_df_header(header_bytes: &[u8]) -> Option<dataforge::DFBinaryHeader> {
    let mut cursor = std::io::Cursor::new(header_bytes);
    dataforge::read_binary_header_sync(&mut cursor).ok()
}

fn region_lengths(header: &dataforge::DFBinaryHeader) -> (usize, usize) {
    match header {
        dataforge::DFBinaryHeader::DF01 {
            meta_len, data_len, ..
        } => (*meta_len, *data_len),
    }
}

fn meta_from_parts(header_bytes: &[u8], meta_bytes: Vec<u8>) -> Option<NumassMeta> {
    let header = read_df_header(header_bytes)?;
    dataforge::parse_meta(&header, meta_bytes).ok()
}

fn point_from_data(data: &[u8]) -> rsb_event::Point {
    rsb_event::Point::parse_from_bytes(data).unwrap()
}

/// Read `len` bytes at `start`. Native code seeks in the local file.
/// Wasm requests the same byte range over HTTP.
async fn read_range(filepath: &Path, start: u64, len: usize) -> Vec<u8> {
    if len == 0 {
        return Vec::new();
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let mut file = tokio::fs::File::open(filepath).await.unwrap();
        file.seek(std::io::SeekFrom::Start(start)).await.unwrap();
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).await.unwrap();
        buf
    }

    #[cfg(target_arch = "wasm32")]
    {
        let end = start + u64::try_from(len).unwrap() - 1;
        let response = gloo::net::http::Request::get(&api_url("files", filepath))
            .header("Range", &format!("bytes={start}-{end}"))
            .send()
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.binary().await.unwrap();
        if status != 206 || bytes.len() != len {
            panic!(
                "range bytes={start}-{end} for {filepath:?}: status {status}, {} bytes",
                bytes.len()
            );
        }
        bytes
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_range_sync(filepath: &Path, start: u64, len: usize) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    if len == 0 {
        return Vec::new();
    }
    let mut file = std::fs::File::open(filepath).unwrap();
    file.seek(SeekFrom::Start(start)).unwrap();
    let mut buf = vec![0u8; len];
    file.read_exact(&mut buf).unwrap();
    buf
}

/// Полный JSON метаданных DF01, без сужения до [`NumassMeta`].
#[cfg(not(target_arch = "wasm32"))]
pub fn load_meta_json_sync(filepath: &Path) -> Result<serde_json::Value, dataforge::DFParseError> {
    let mut file = std::fs::File::open(filepath)?;
    Ok(dataforge::read_df_header_and_meta_sync::<serde_json::Value>(&mut file)?.1)
}

/// Load point metadata only from the storage.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_meta_sync(filepath: &Path) -> Option<NumassMeta> {
    let header_bytes = read_range_sync(filepath, 0, DF_HEADER_LEN);
    let header = read_df_header(&header_bytes)?;
    let (meta_len, _) = region_lengths(&header);
    let meta_bytes = read_range_sync(filepath, DF_HEADER_LEN as u64, meta_len);
    meta_from_parts(&header_bytes, meta_bytes)
}

/// Load point metadata only from the storage.
pub async fn load_meta(filepath: &Path) -> Option<NumassMeta> {
    let header_bytes = read_range(filepath, 0, DF_HEADER_LEN).await;
    let header = read_df_header(&header_bytes)?;
    let (meta_len, _) = region_lengths(&header);
    let meta_bytes = read_range(filepath, DF_HEADER_LEN as u64, meta_len).await;
    meta_from_parts(&header_bytes, meta_bytes)
}

pub async fn load_modified_time(filepath: PathBuf) -> Option<SystemTime> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::metadata(filepath)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
    }

    #[cfg(target_arch = "wasm32")]
    {
        propfind(&filepath, "0")
            .await
            .and_then(|entries| entries.into_iter().find_map(|entry| entry.modified))
    }
}

/// Load and parse point binary data from the storage.
/// Do not use this function directly without reason.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_point_sync(filepath: &Path) -> rsb_event::Point {
    let header_bytes = read_range_sync(filepath, 0, DF_HEADER_LEN);
    let header = read_df_header(&header_bytes).unwrap();
    let (meta_len, data_len) = region_lengths(&header);
    let data = read_range_sync(
        filepath,
        DF_HEADER_LEN as u64 + u64::try_from(meta_len).unwrap(),
        data_len,
    );
    point_from_data(&data)
}

/// Load and parse point binary data from the storage.
/// Do not use this function directly without reason.
pub async fn load_point(filepath: &Path) -> rsb_event::Point {
    let header_bytes = read_range(filepath, 0, DF_HEADER_LEN).await;
    let header = read_df_header(&header_bytes).unwrap();
    let (meta_len, data_len) = region_lengths(&header);
    let data = read_range(
        filepath,
        DF_HEADER_LEN as u64 + u64::try_from(meta_len).unwrap(),
        data_len,
    )
    .await;
    point_from_data(&data)
}

/// Temporal numass file storage representation.
/// TODO: switch to real numass storage service when it will be implemented.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum LoadState {
    #[default]
    NotLoaded,
    NeedLoad,
    Loaded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FSRepr {
    File {
        path: PathBuf,
        modified: SystemTime,
    },
    Directory {
        path: PathBuf,
        children: Vec<FSRepr>,
        modified: SystemTime,
        #[serde(skip_serializing, default)]
        load_state: LoadState,
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
            modified: SystemTime::UNIX_EPOCH,
        }
    }

    pub async fn reload(self) {
        unimplemented!()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn ls(path: PathBuf) -> FSRepr {
        let meta = tokio::fs::metadata(&path).await.unwrap();

        if meta.is_file() {
            return FSRepr::File {
                path,
                modified: meta.modified().unwrap(),
            };
        }

        if meta.is_dir() {
            let mut read_dir = tokio::fs::read_dir(&path).await.unwrap();

            let mut children = vec![];
            while let Ok(Some(child)) = read_dir.next_entry().await {
                let meta = child.metadata().await.unwrap();
                let path = child.path();
                if meta.is_file() {
                    children.push(FSRepr::File {
                        path,
                        modified: meta.modified().unwrap(),
                    });
                } else if meta.is_dir() {
                    children.push(FSRepr::Directory {
                        path,
                        children: vec![],
                        modified: meta.modified().unwrap(),
                        load_state: LoadState::NotLoaded,
                    });
                }
            }
            FSRepr::Directory {
                path,
                children,
                modified: meta.modified().unwrap(),
                load_state: LoadState::NotLoaded,
            }
        } else {
            panic!("neither file, nor directory!")
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn ls(path: PathBuf) -> FSRepr {
        let entries = propfind(&path, "1")
            .await
            .unwrap_or_else(|| panic!("PROPFIND {path:?}"));
        let self_entry = entries.iter().find(|entry| entry.path == path);
        let modified = self_entry
            .and_then(|entry| entry.modified)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        if self_entry.is_some_and(|entry| !entry.is_dir) {
            return FSRepr::File { path, modified };
        }

        let children = entries
            .into_iter()
            .filter(|entry| entry.path != path)
            .map(|entry| {
                let modified = entry.modified.unwrap_or(SystemTime::UNIX_EPOCH);
                if entry.is_dir {
                    FSRepr::Directory {
                        path: entry.path,
                        children: vec![],
                        modified,
                        load_state: LoadState::NotLoaded,
                    }
                } else {
                    FSRepr::File {
                        path: entry.path,
                        modified,
                    }
                }
            })
            .collect();

        FSRepr::Directory {
            path,
            children,
            modified,
            load_state: LoadState::NotLoaded,
        }
    }

    pub async fn expand(
        path: PathBuf,
        children: &mut Vec<FSRepr>,
        modified: &mut SystemTime,
        load_state: &mut LoadState,
    ) {
        *load_state = LoadState::Loaded;
        let updated = FSRepr::ls(path.to_owned()).await;
        if let FSRepr::Directory {
            children: children_upd,
            modified: modified_upd,
            ..
        } = updated
        {
            let mut to_merge = children_upd
                .into_iter()
                .map(|child| (child.to_filename(), child))
                .collect::<std::collections::HashMap<_, _>>();
            for child in children.iter() {
                if let Some(place) = to_merge.get_mut(&child.to_filename()) {
                    *place = child.clone() // TODO: update modified time in children
                }
            }
            *children = to_merge.into_values().collect::<Vec<_>>();
            children.sort_by(|v1, v2| {
                natord::compare(
                    v1.to_filename().as_os_str().to_str().unwrap(),
                    v2.to_filename().as_os_str().to_str().unwrap(),
                )
            });

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
                if let FSRepr::Directory {
                    path,
                    children,
                    modified,
                    load_state,
                } = child
                {
                    match load_state {
                        LoadState::NeedLoad => {
                            FSRepr::expand(path.to_path_buf(), children, modified, load_state)
                                .await;
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
                if let FSRepr::Directory {
                    path,
                    children,
                    modified,
                    load_state,
                } = leaf
                {
                    match load_state {
                        LoadState::Loaded => {
                            if let FSRepr::Directory {
                                children: mut children_new,
                                modified: modified_new,
                                ..
                            } = FSRepr::ls(path.to_owned()).await
                            {
                                children_new.iter_mut().for_each(|child_new| {
                                    if let Some(child) = children.iter().find(|child| {
                                        child.to_filename() == child_new.to_filename()
                                    }) && let (
                                        FSRepr::Directory {
                                            load_state: LoadState::Loaded,
                                            ..
                                        },
                                        FSRepr::Directory { load_state, .. },
                                    ) = (child, child_new)
                                    {
                                        *load_state = LoadState::Loaded;
                                    }
                                });
                                children_new.sort_by(|v1, v2| {
                                    natord::compare(
                                        v1.to_filename().as_os_str().to_str().unwrap(),
                                        v2.to_filename().as_os_str().to_str().unwrap(),
                                    )
                                });
                                *children = children_new;
                                *modified = modified_new;
                            }

                            for ele in children.iter_mut() {
                                next.push(ele);
                            }
                        }
                        LoadState::NeedLoad => {}
                        LoadState::NotLoaded => {}
                    }
                }
            }
            current = next;
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct ListingEntry {
    path: PathBuf,
    modified: Option<SystemTime>,
    is_dir: bool,
}

/// Depth `0` or `1` PROPFIND of `filepath` under `/files`.
/// `None` when the resource is missing or the response is not 207.
#[cfg(target_arch = "wasm32")]
async fn propfind(filepath: &Path, depth: &str) -> Option<Vec<ListingEntry>> {
    const BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:">
  <D:prop>
    <D:getlastmodified/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;

    let response = gloo::net::http::Request::get(&api_url("files", filepath))
        .method(gloo::net::http::Method::from_bytes(b"PROPFIND").unwrap())
        .header("Depth", depth)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(BODY)
        .unwrap()
        .send()
        .await
        .ok()?;
    if response.status() != 207 {
        return None;
    }
    let xml = response.text().await.ok()?;
    Some(parse_propfind(&xml))
}

#[cfg(target_arch = "wasm32")]
fn parse_propfind(xml: &str) -> Vec<ListingEntry> {
    let root = xmltree::Element::parse(xml.as_bytes()).unwrap();
    root.children
        .iter()
        .filter_map(|node| match node {
            xmltree::XMLNode::Element(response) if response.name == "response" => Some(response),
            _ => None,
        })
        .map(|response| {
            let href = response
                .get_child("href")
                .and_then(|node| node.get_text())
                .unwrap_or_default();
            ListingEntry {
                path: href_storage_path(&href),
                modified: prop_text(response, "getlastmodified")
                    .and_then(|value| http_date(&value)),
                is_dir: prop_element(response, "resourcetype")
                    .and_then(|node| node.get_child("collection"))
                    .is_some(),
            }
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn prop_element<'a>(response: &'a xmltree::Element, name: &str) -> Option<&'a xmltree::Element> {
    response
        .children
        .iter()
        .filter_map(|node| match node {
            xmltree::XMLNode::Element(element) if element.name == "propstat" => {
                element.get_child("prop")
            }
            _ => None,
        })
        .find_map(|prop| prop.get_child(name))
}

#[cfg(target_arch = "wasm32")]
fn prop_text(response: &xmltree::Element, name: &str) -> Option<String> {
    prop_element(response, name)
        .and_then(|node| node.get_text())
        .map(|text| text.trim().to_string())
}

/// Turn a PROPFIND href into a path relative to the storage root.
/// Href is either `/files/...` or an absolute URL with that path.
#[cfg(target_arch = "wasm32")]
fn href_storage_path(href: &str) -> PathBuf {
    let without_query = href.split(['?', '#']).next().unwrap_or(href);
    let path = if let Some((_, rest)) = without_query.split_once("://") {
        rest.split_once('/').map(|(_, path)| path).unwrap_or("")
    } else {
        without_query.trim_start_matches('/')
    };
    let decoded = urlencoding::decode(path).unwrap_or(std::borrow::Cow::Borrowed(path));
    let mut segments: Vec<&str> = decoded.split('/').filter(|part| !part.is_empty()).collect();
    if segments.first() == Some(&"files") {
        segments.remove(0);
    }
    PathBuf::from(segments.join("/"))
}

#[cfg(target_arch = "wasm32")]
fn http_date(value: &str) -> Option<SystemTime> {
    let parsed = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let timestamp = parsed.timestamp();
    if timestamp < 0 {
        return None;
    }
    Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64))
}
