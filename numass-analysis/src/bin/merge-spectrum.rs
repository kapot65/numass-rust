use std::sync::Arc;

use analysis::get_points_by_pattern;

use processing::{
    histogram::PointHistogram,
    postprocess::{post_process, PostProcessParams},
    process::{ProcessParams, TRAPEZOID_DEFAULT},
    storage::process_point,
    types::FrameEvent,
};

use tokio::sync::Mutex;

use unzip_n::unzip_n;

#[cfg(target_family = "unix")]
use tikv_jemallocator::Jemalloc;
#[cfg(target_family = "unix")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

unzip_n!(pub 4);

#[tokio::main(worker_threads = 3)]

async fn main() {
    let db_root = "/data-nvme/";

    let pattern = "/2024_03/Tritium_*/set_*/p*(30s)(HV1=11000)";
    let exclude = [];

    let points = get_points_by_pattern(db_root, pattern, &exclude)
        .first_key_value()
        .unwrap()
        .1
        .clone();

    // let coeffs = Arc::new(CorrectionCoeffs::load(
    //     "/data-3/numass-server/monitor_test.json",
    // ));

    let pb: Arc<Mutex<indicatif::ProgressBar>> =
        Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));

    let hist = Arc::new(Mutex::new(PointHistogram::new(0.0..60.0, 300)));

    let handles = points
        .iter()
        .map(|filepath| {
            let filepath = filepath.clone();

            let hist = Arc::clone(&hist);

            let pb = Arc::clone(&pb);

            tokio::spawn(async move {
                let events = post_process(
                    process_point(
                        &filepath,
                        &ProcessParams {
                            algorithm: TRAPEZOID_DEFAULT,
                            convert_to_kev: true,
                        },
                    )
                    .await
                    .unwrap()
                    .1
                    .unwrap(),
                    &PostProcessParams {
                        merge_splits_first: true,
                        merge_close_events: true,
                        ignore_borders: false,
                    },
                );

                let mut hist = hist.lock().await;
                events.into_iter().for_each(|(_, frames)| {
                    frames.into_iter().for_each(|(_, frame)| {
                        if let FrameEvent::Event {
                            channel, amplitude, ..
                        } = frame
                        {
                            hist.add(channel, amplitude);
                        }
                    });
                });
                pb.lock().await.inc(1);
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    std::fs::write("aggregated.tsv", hist.lock().await.to_csv('\t')).unwrap();
}
