//! Подготовка данных для поиска корреляций в отколнении спектра
//!

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use indicatif::ProgressStyle;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use analysis::CorrectionCoeffs;
use processing::{
    numass::{NumassMeta, Reply},
    process::{ProcessParams, TRAPEZOID_DEFAULT},
    storage::process_point,
    types::FrameEvent,
};

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ProducedPoint {
    u_sp: u16,
    l_curr: f32,
    k: f64,        // 5..19.5 keV
    l: f64,        // 5..[(U_sp -11000)/1000 +2] keV
    doubles: f64,  // 19.5..33.5 keV (двойные)
    tripples: f64, // 33.5..50 keV (тройные)
    triggers: usize,
}

#[tokio::main] // TODO adjust worker_threads ( #[tokio::main(worker_threads = ?)] )
async fn main() {
    let corr_pattern = "p174(30s)(HV1=11000)";
    let monitor_pattern = "p175(30s)(HV1=14000)";

    let coeffs_path = "/data-3/numass-server/monitor_test.json";

    let algorithm = TRAPEZOID_DEFAULT;

    let sets = [
        "/data-2/numass-server/2024_03/Tritium_3/set_2",
        "/data-2/numass-server/2024_03/Tritium_3/set_3",
        "/data-2/numass-server/2024_03/Tritium_3/set_4",
        "/data-2/numass-server/2024_03/Tritium_3/set_5",
        "/data-2/numass-server/2024_03/Tritium_3/set_6",
        "/data-2/numass-server/2024_03/Tritium_3/set_7",
        "/data-2/numass-server/2024_03/Tritium_3/set_8",
        "/data-2/numass-server/2024_03/Tritium_4/set_1",
        "/data-2/numass-server/2024_03/Tritium_4/set_2",
        "/data-2/numass-server/2024_03/Tritium_4/set_3",
        "/data-2/numass-server/2024_03/Tritium_4/set_4",
        "/data-2/numass-server/2024_03/Tritium_4/set_5",
        "/data-2/numass-server/2024_03/Tritium_4/set_6",
        "/data-2/numass-server/2024_03/Tritium_5/set_1",
        "/data-2/numass-server/2024_03/Tritium_5/set_2",
        "/data-2/numass-server/2024_03/Tritium_5/set_3",
        "/data-2/numass-server/2024_03/Tritium_5/set_4",
        "/data-2/numass-server/2024_03/Tritium_5/set_5",
        "/data-2/numass-server/2024_03/Tritium_5/set_6",
        "/data-2/numass-server/2024_03/Tritium_6/set_1",
        "/data-2/numass-server/2024_03/Tritium_6/set_2",
        "/data-2/numass-server/2024_03/Tritium_6/set_3",
        "/data-2/numass-server/2024_03/Tritium_6/set_4",
        "/data-2/numass-server/2024_03/Tritium_6/set_5",
        "/data-2/numass-server/2024_03/Tritium_6/set_6",
        "/data-2/numass-server/2024_03/Tritium_6/set_7",
        "/data-2/numass-server/2024_03/Tritium_6/set_8",
        "/data-2/numass-server/2024_03/Tritium_6/set_9",
        "/data-2/numass-server/2024_03/Tritium_6/set_10",
        "/data-2/numass-server/2024_03/Tritium_6/set_11",
        "/data-2/numass-server/2024_03/Tritium_6/set_12",
        "/data-2/numass-server/2024_03/Tritium_6/set_13",
        "/data-2/numass-server/2024_03/Tritium_7/set_1",
        "/data-2/numass-server/2024_03/Tritium_7/set_2",
        "/data-2/numass-server/2024_03/Tritium_7/set_3",
        "/data-2/numass-server/2024_03/Tritium_7/set_4",
        "/data-2/numass-server/2024_03/Tritium_7/set_5",
        "/data-2/numass-server/2024_03/Tritium_7/set_6",
        "/data-2/numass-server/2024_03/Tritium_7/set_7",
        "/data-2/numass-server/2024_03/Tritium_7/set_8",
        "/data-2/numass-server/2024_03/Tritium_7/set_9",
        "/data-2/numass-server/2024_03/Tritium_7/set_10",
        "/data-2/numass-server/2024_03/Tritium_7/set_11",
        "/data-2/numass-server/2024_03/Tritium_7/set_12",
        "/data-2/numass-server/2024_03/Tritium_7/set_13",
        "/data-2/numass-server/2024_03/Tritium_7/set_14",
        "/data-2/numass-server/2024_03/Tritium_7/set_15",
        "/data-2/numass-server/2024_03/Tritium_7/set_16",
        "/data-2/numass-server/2024_03/Tritium_7/set_17",
        "/data-2/numass-server/2024_03/Tritium_7/set_18",
        "/data-2/numass-server/2024_03/Tritium_7/set_19",
        "/data-2/numass-server/2024_03/Tritium_8/set_1",
        "/data-2/numass-server/2024_03/Tritium_8/set_2",
        "/data-2/numass-server/2024_03/Tritium_8/set_3",
        "/data-3/numass-server/2024_03/Tritium_8/set_4",
        "/data-3/numass-server/2024_03/Tritium_8/set_5",
        "/data-3/numass-server/2024_03/Tritium_8/set_6",
        "/data-3/numass-server/2024_03/Tritium_8/set_7",
        "/data-3/numass-server/2024_03/Tritium_8/set_8",
        "/data-3/numass-server/2024_03/Tritium_8/set_9",
        "/data-3/numass-server/2024_03/Tritium_9/set_1",
        "/data-3/numass-server/2024_03/Tritium_9/set_2",
        "/data-3/numass-server/2024_03/Tritium_9/set_3",
        "/data-3/numass-server/2024_03/Tritium_9/set_4",
        "/data-3/numass-server/2024_03/Tritium_9/set_5",
        "/data-3/numass-server/2024_03/Tritium_9/set_6",
        "/data-3/numass-server/2024_03/Tritium_9/set_7",
        "/data-3/numass-server/2024_03/Tritium_9/set_8",
        "/data-3/numass-server/2024_03/Tritium_9/set_9",
        "/data-3/numass-server/2024_03/Tritium_10/set_1",
        "/data-3/numass-server/2024_03/Tritium_10/set_2",
        "/data-3/numass-server/2024_03/Tritium_10/set_3",
        "/data-3/numass-server/2024_03/Tritium_10/set_4",
        "/data-3/numass-server/2024_03/Tritium_10/set_5",
        "/data-3/numass-server/2024_03/Tritium_10/set_6",
        "/data-3/numass-server/2024_03/Tritium_10/set_7",
        "/data-3/numass-server/2024_03/Tritium_10/set_8",
        "/data-3/numass-server/2024_03/Tritium_11/set_1",
        "/data-3/numass-server/2024_03/Tritium_11/set_3",
        "/data-3/numass-server/2024_03/Tritium_11/set_4",
        "/data-3/numass-server/2024_03/Tritium_11/set_5",
        "/data-3/numass-server/2024_03/Tritium_11/set_6",
        "/data-3/numass-server/2024_03/Tritium_11/set_7",
        "/data-3/numass-server/2024_03/Tritium_12/set_1",
        "/data-3/numass-server/2024_03/Tritium_12/set_2",
        "/data-3/numass-server/2024_03/Tritium_12/set_3",
        "/data-3/numass-server/2024_03/Tritium_12/set_4",
        "/data-3/numass-server/2024_03/Tritium_12/set_5",
        "/data-3/numass-server/2024_03/Tritium_12/set_6",
        "/data-3/numass-server/2024_03/Tritium_12/set_7",
        "/data-3/numass-server/2024_03/Tritium_12/set_8",
        "/data-3/numass-server/2024_03/Tritium_12/set_9",
    ];

    let coeffs = Arc::new(CorrectionCoeffs::load(coeffs_path));

    let pb = indicatif::ProgressBar::new(sets.len() as u64);
    pb.set_style(
        ProgressStyle::with_template("[{elapsed_precise}] {bar} {pos:>7}/{len:7} {msg}").unwrap(),
    );
    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(pb));

    let table = Arc::new(Mutex::new(BTreeMap::new()));

    let handles = sets
        .into_iter()
        .map(|set| {
            let algorithm = algorithm.clone();
            let coeffs = coeffs.clone();
            let table = Arc::clone(&table);
            let pb = Arc::clone(&pb);

            tokio::spawn(async move {
                // let mut hist = PointHistogram::new(hist_params.range, hist_params.bins);

                let corr_point = PathBuf::from(set).join(corr_pattern);

                // let pp = StaticProcessParams::from_point(&load_point(&corr_point).await);
                // let baseline = pp.baseline.unwrap().iter().sum::<f32>();

                let monitor_coeff = coeffs.get_by_index(&corr_point) as f64;

                let (corr_meta, corr_frames) = process_point(
                    &corr_point,
                    &ProcessParams {
                        algorithm: algorithm.clone(),
                        convert_to_kev: true,
                    },
                )
                .await
                .unwrap();
                let corr_frames = corr_frames.unwrap();

                let monitor_point = PathBuf::from(set).join(monitor_pattern);
                let (_, monitor_frames) = process_point(
                    &monitor_point,
                    &ProcessParams {
                        algorithm,
                        convert_to_kev: true,
                    },
                )
                .await
                .unwrap();
                let monitor_frames = monitor_frames.unwrap();

                let mut k_c = 0u64;
                let mut l_c = 0u64;

                corr_frames.iter().for_each(|(_, events)| {
                    events.iter().for_each(|(_, event)| {
                        if let FrameEvent::Event {
                            amplitude, ..
                        } = event
                        {
                            // if channel != &1 && channel != &5 {
                            if (4.0..19.5).contains(amplitude) {
                                k_c += 1;
                                if (4.0..6.0).contains(amplitude) {
                                    l_c += 1;
                                }
                            }
                            // }
                        }
                    })
                });

                let mut k_m = 0u64;
                let mut l_m = 0u64;

                monitor_frames.iter().for_each(|(_, events)| {
                    events.iter().for_each(|(_, event)| {
                        if let FrameEvent::Event {
                            amplitude, ..
                        } = event
                        {
                            // if channel != &1 && channel != &5 {
                            if (4.0..19.5).contains(amplitude) {
                                k_m += 1;
                                if (4.0..9.0).contains(amplitude) {
                                    l_m += 1;
                                }
                            }
                            // }
                        }
                    })
                });

                let time =
                    if let NumassMeta::Reply(Reply::AcquirePoint { start_time, .. }) = corr_meta {
                        start_time
                    } else {
                        panic!("wrong message type")
                    };

                table
                    .lock()
                    .await
                    // .insert(time, (k_c, l_c, k_m, l_m, baseline, monitor_coeff))
                    .insert(time, (k_c, l_c, k_m, l_m, monitor_coeff));
                pb.lock().await.inc(1)
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    // let mut table_data = "timestamp\tk_c\tl_c\tk_m\tl_m\tbaseline\tmonitor_coeff\n".to_string();
    // table.try_lock().unwrap().clone().iter().for_each(
    //     |(timestamp, (k_c, l_c, k_m, l_m, baseline, monitor_coeff))| {
    //         table_data.push_str(&format!(
    //             "{timestamp}\t{k_c}\t{l_c}\t{k_m}\t{l_m}\t{baseline}\t{monitor_coeff}\n"
    //         ));
    //     },
    // );

    let mut table_data = "timestamp\tk_c\tl_c\tk_m\tl_m\tmonitor_coeff\n".to_string();
    table.try_lock().unwrap().clone().iter().for_each(
        |(timestamp, (k_c, l_c, k_m, l_m, monitor_coeff))| {
            table_data.push_str(&format!(
                "{timestamp}\t{k_c}\t{l_c}\t{k_m}\t{l_m}\t{monitor_coeff}\n"
            ));
        },
    );

    std::fs::write("for-corr-prepated.tsv", table_data).unwrap()
}
