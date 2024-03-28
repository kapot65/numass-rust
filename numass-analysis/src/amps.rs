use std::{collections::hash_map::DefaultHasher, hash::{Hash, Hasher}, path::PathBuf};
use crate::workspace::get_cache_root;

use super::cache::CacacheBackend;
use cached::proc_macro::io_cached;
use eyre::{Result, eyre};
use processing::{types::NumassEvents, process::ProcessParams};

/// do extract_amplitudes for given point or get it from cache
#[io_cached(
    map_error = r##"|e| e"##,
    type = "CacacheBackend<u64, NumassEvents>",
    create = r#"{ CacacheBackend::new(get_cache_root().join("extract_events")) }"#, // TODO: make it configurable
    convert = r#"{ {
        let mut hasher = DefaultHasher::new();
        filepath.hash(&mut hasher);
        params.hash(&mut hasher);
        hasher.finish()
    } }"#
)]
pub async fn get_amps(filepath: &PathBuf, params: &ProcessParams) -> Result<NumassEvents> {

    // TODO: implement relative files + fast/slow db selection
    // let filepath = {
    //     // first try to find point in fast db
    //     let filepath_fast = get_db_fast_root().join(point);
    //     if tokio::fs::try_exists(&filepath_fast).await? {
    //         filepath_fast
    //     } else {
    //         get_db_slow_root().join(point)
    //     }
    // };

    let events = processing::storage::process_point(filepath, params).await;
    events.ok_or(eyre!("{filepath:?}, process_point returns None"))?.1
        .ok_or(eyre!("{filepath:?} binary data is empty"))
}