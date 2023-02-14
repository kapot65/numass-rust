use std::{path::PathBuf, time::SystemTime};

#[cfg(not(target_arch = "wasm32"))]
use dataforge::read_df_message;
#[cfg(not(target_arch = "wasm32"))]
use numass::{NumassMeta, protos::rsb_event, Reply};
#[cfg(not(target_arch = "wasm32"))]
use protobuf::Message;
#[cfg(not(target_arch = "wasm32"))]
use processing::point_to_histogramm;

use processing::{ProcessingParams, histogram::PointHistogram};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FSRepr {
    File {
        path: PathBuf,
    },
    Directory {
        path: PathBuf,
        children: Vec<FSRepr>
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCache {
    pub opened: bool,
    pub processed: Option<SystemTime>,
    pub histogram: Option<PointHistogram>
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub filepath: PathBuf,
    pub params: ProcessingParams
}

impl FSRepr {
    pub fn to_filename(&self) -> &str{
        let path = match self {
            FSRepr::File { path }  => path,
            FSRepr::Directory { path, children: _ } => path
        };
        path.file_name().unwrap().to_str().unwrap()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn process_file(filepath: PathBuf, params: ProcessingParams) -> FileCache {
    let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
    let message = read_df_message::<NumassMeta>(&mut point_file).await.unwrap();
    match message.meta {
        numass::NumassMeta::Reply(Reply::AcquirePoint { ..
        }) => {
            let data = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
            let processed = std::fs::metadata(&filepath).unwrap().modified().unwrap();
            
            FileCache {
                opened: true,
                histogram: Some(point_to_histogramm(&data, params)),
                processed: Some(processed)
            }
        }
        _ => {
            panic!()
        }
    }
}

pub fn expand_dir(path: PathBuf) -> FSRepr {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let meta = std::fs::metadata(&path).unwrap();
        if meta.is_file() {
            FSRepr::File { path }

        } else if meta.is_dir() {
            let children = std::fs::read_dir(&path).unwrap();

            let mut children = children.map(|child| {
                let entry = child.unwrap();
                expand_dir(entry.path())
            }).collect::<Vec<_>>();

            children.sort_by(|a, b| {
                natord::compare(a.to_filename(), b.to_filename())
            });

            FSRepr::Directory { path, children }
        } else {
            panic!()
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        todo!()
    }
    
}