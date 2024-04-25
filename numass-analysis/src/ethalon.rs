use std::{sync::Arc, collections::hash_map::DefaultHasher, hash::{Hash, Hasher}};
use crate::{workspace::{get_cache_root, get_db_fast_root, get_hist_range, get_hist_bins}, amps::get_amps};

use super::cache::CacacheBackend;
use tokio::sync::Mutex;
use cached::proc_macro::io_cached;
use eyre::Result;
use processing::{histogram::PointHistogram, postprocess::{post_process, PostProcessParams}, process::ProcessParams, types::FrameEvent};


/// Calculate ethalon histogram for given pattern or get it from cache
#[io_cached(
    map_error = r##"|e| e"##,
    type = "CacacheBackend<u64, PointHistogram>",
    create = r#"{ CacacheBackend::new(get_cache_root().join("ethalon")) }"#, // TODO: make it configurable
    convert = r#"{ {
        let mut hasher = DefaultHasher::new();
        pattern.hash(&mut hasher);
        process_params.hash(&mut hasher);
        post_process_params.hash(&mut hasher);
        hasher.finish()
    } }"#
)]
pub async fn get_ethalon(
        pattern: String, 
        process_params: ProcessParams, 
        post_process_params: PostProcessParams
    ) -> Result<PointHistogram> {

    let pattern = get_db_fast_root().join(&pattern).to_str().unwrap().to_string();

    let range = get_hist_range();
    let bins = get_hist_bins();

    let eth_hist = {
        let eth_points = {
            let exclude: Vec<String> = vec![];
            super::get_points_by_pattern(
                get_db_fast_root().to_str().unwrap(), 
                &pattern, &exclude)
        };
    
        let hist = Arc::new(Mutex::new(
            PointHistogram::new(range.clone(), bins)
        ));
    
        let handles = eth_points.iter().flat_map(|(_, filepaths)| {
            filepaths.iter().map(|filepath| {
                let hist = Arc::clone(&hist);
                let filepath = filepath.clone();
                let process_params = process_params.clone();

                tokio::spawn(async move {
                    let amps = post_process(
                        get_amps(&filepath, &process_params).await.unwrap(),
                        &post_process_params
                    );

                    {
                        let mut histogram = hist.lock().await;
                        amps.into_iter().for_each(|(_, block)| {
                            block.into_iter().for_each(|(_, event)| {
                                if let FrameEvent::Event { channel, amplitude, .. } = event {
                                    histogram.add(channel, amplitude);
                                }
                            });
                        });
                    }
                })
            })
        }).collect::<Vec<_>>();
        for handle in handles {
            handle.await.unwrap();
        }
        hist
    };
    let eth_hist = eth_hist.lock().await;

    Ok(eth_hist.clone())
}