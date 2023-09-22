use std::{path::PathBuf, collections::{BTreeMap, hash_map::DefaultHasher}, hash::{Hash, Hasher}};
use crate::workspace::{get_workspace, get_db_fast_root, get_db_slow_root};

use super::cache::CacacheBackend;
use dataforge::read_df_message;
use protobuf::Message;
use cached::proc_macro::io_cached;
use eyre::Result;
use processing::{numass::{NumassMeta, protos::rsb_event}, extract_amplitudes, ProcessParams};

/// do extract_amplitudes for given point or get it from cache
#[io_cached(
    map_error = r##"|e| e"##,
    type = "CacacheBackend<u64, BTreeMap<u64, BTreeMap<usize, f32>>>",
    create = r#"{ CacacheBackend::new(get_workspace().join("cache/extract_amplitudes")) }"#, // TODO: make it configurable
    convert = r#"{ {
        let mut hasher = DefaultHasher::new();
        params.hash(&mut hasher);
        point.hash(&mut hasher);
        hasher.finish()
    } }"#
)]
pub async fn get_amps(point: &PathBuf, params: ProcessParams) -> Result<BTreeMap<u64, BTreeMap<usize, f32>>> {

    let filepath = {
        // first try to find point in fast db
        let filepath_fast = get_db_fast_root().join(point);
        if tokio::fs::try_exists(&filepath_fast).await? {
            filepath_fast
        } else {
            get_db_slow_root().join(point)
        }
    };

    let mut point_file = tokio::fs::File::open(filepath).await?;
    let message = read_df_message::<NumassMeta>(&mut point_file)
        .await
        .unwrap();
    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let amps = extract_amplitudes(
        &point, 
        &params
    );

    Ok(amps)
}