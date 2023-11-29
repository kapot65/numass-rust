use std::{path::PathBuf, collections::hash_map::DefaultHasher, hash::{Hash, Hasher}};
use crate::workspace::get_cache_root;

use super::cache::CacacheBackend;
use dataforge::read_df_message;
use protobuf::Message;
use cached::proc_macro::io_cached;
use eyre::Result;
use processing::{numass::{NumassMeta, protos::rsb_event}, NumassAmps, extract_events, ProcessParams};

/// do extract_amplitudes for given point or get it from cache
#[io_cached(
    map_error = r##"|e| e"##,
    type = "CacacheBackend<u64, NumassAmps>",
    create = r#"{ CacacheBackend::new(get_cache_root().join("extract_events")) }"#, // TODO: make it configurable
    convert = r#"{ {
        let mut hasher = DefaultHasher::new();
        filepath.hash(&mut hasher);
        params.hash(&mut hasher);
        hasher.finish()
    } }"#
)]
pub async fn get_amps(filepath: &PathBuf, params: &ProcessParams) -> Result<NumassAmps> {

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

    let mut point_file = tokio::fs::File::open(filepath).await?;
    let message = read_df_message::<NumassMeta>(&mut point_file)
        .await
        .unwrap();
    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

    let amps = extract_events(
        &point, 
        params
    );

    Ok(amps)
}