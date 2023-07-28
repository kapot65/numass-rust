use std::{path::PathBuf, sync::Arc};
use super::cache::CacacheBackend;
use dataforge::read_df_message;
use protobuf::Message;
use tokio::sync::Mutex;
use cached::proc_macro::io_cached;
use eyre::Result;
use processing::{histogram::PointHistogram, numass::{NumassMeta, protos::rsb_event}, extract_amplitudes, ProcessParams};

/// Calculate ethalon histogram for given voltage or get it from cache
/// ! most parameters are hardcoded
#[io_cached(
    map_error = r##"|e| e"##,
    type = "CacacheBackend<String, PointHistogram>",
    create = r#"{ CacacheBackend::new(PathBuf::from("workspace/cache")) }"#, // TODO: make it configurable
    convert = r#"{ format!("get_ethalon({voltage})") }"#
)]
pub async fn get_ethalon(voltage: u16) -> Result<PointHistogram> {

    let db_root = "/data-nvme"; // TODO: make it configurable
    let run = "2023_03"; // TODO: make it configurable
    let range = 2.0..20.0; // TODO: make it configurable
    let bins = 180; // TODO: make it configurable

    let ethalon_pattern = format!("/{run}/Tritium_1/set_[1234]/p*(HV1={voltage})");

    let eth_hist = {
        let eth_points = {
            
            let exclude: Vec<String> = vec![];
            super::get_points_by_pattern(db_root, &ethalon_pattern, &exclude)
        };
    
        let hist = Arc::new(Mutex::new(
            PointHistogram::new(range.clone(), bins)
        ));
    
        let handles = eth_points.iter().flat_map(|(_, filepaths)| {
            filepaths.iter().map(|filepath| {
                let hist = Arc::clone(&hist);
                let filepath = filepath.clone();
                tokio::spawn(async move {
                    let mut point_file = tokio::fs::File::open(filepath).await.unwrap();
                    let message = read_df_message::<NumassMeta>(&mut point_file)
                        .await
                        .unwrap();
                    let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
                    
                    let amps = extract_amplitudes(
                        &point, 
                        &ProcessParams::default()
                    );

                    {
                        let mut hist = hist.lock().await;
                        amps.into_iter().for_each(|(_, block)| {
                            block.into_iter().for_each(|(ch_num, amp)| {
                                hist.add(ch_num as u8, amp);
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