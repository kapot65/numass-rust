use std::{collections::BTreeMap, sync::Arc};

use analysis::{get_points_by_pattern, CorrectionCoeffs};

use dataforge::read_df_header_and_meta_sync;

use plotly::{
    common::{Mode, Title},
    layout::Axis,
    Layout, Plot, Scatter,
};

use processing::{
    numass::{NumassMeta, Reply},
    postprocess::{post_process, PostProcessParams},
    process::{extract_events, ProcessParams, TRAPEZOID_DEFAULT},
    storage::load_point,
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

#[tokio::main]
async fn main() {
    let db_root = "/data-fast/numass-server";

    let pattern = "2024_11/Tritium_4/set_*[0-9]/p*(30s)(HV1=14000)";
    let exclude = [];

    let monitor_file = "/data-fast/monitor_11_2024.json";

    let points = get_points_by_pattern(db_root, pattern, &exclude)
        .first_key_value()
        .unwrap()
        .1
        .clone();

    let coeffs = Arc::new(CorrectionCoeffs::load(monitor_file));

    let count_rates = Arc::new(Mutex::new(BTreeMap::new()));

    let pb: Arc<Mutex<indicatif::ProgressBar>> =
        Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));

    let handles = points
        .iter()
        .map(|filepath| {
            let filepath = filepath.clone();

            let count_rates = Arc::clone(&count_rates);

            let pb = Arc::clone(&pb);

            let coeffs = Arc::clone(&coeffs);

            tokio::spawn(async move {
                let (_, meta) = read_df_header_and_meta_sync::<NumassMeta>(
                    &mut std::fs::File::open(&filepath).unwrap(),
                )
                .unwrap();

                let k = coeffs.get_by_index(&filepath);

                let point = load_point(&filepath).await;

                let (amps, preprocess) = post_process(
                    extract_events(
                        Some(meta.clone()),
                        point,
                        &ProcessParams {
                            algorithm: TRAPEZOID_DEFAULT,
                            convert_to_kev: true,
                        },
                    ),
                    &PostProcessParams::default(),
                );

                let count_rate = amps
                    .values()
                    .map(|frames| {
                        frames
                            .iter()
                            .filter(|(_, event)| {
                                if let FrameEvent::Event { amplitude, .. } = event {
                                    return (6.0..40.0).contains(amplitude);
                                }
                                false
                                // TODO: check if it's correct
                            })
                            .collect::<Vec<_>>()
                            .len()
                    })
                    .sum::<usize>();

                let acq_duration = (preprocess.effective_time() / 1_000_000_000) as f32;

                if let NumassMeta::Reply(Reply::AcquirePoint { start_time, .. }) = meta {
                    count_rates.lock().await.insert(
                        start_time,
                        (
                            count_rate as f32 / acq_duration,
                            count_rate as f32 * k / acq_duration,
                            filepath.to_str().unwrap().to_owned(),
                        ),
                    );
                } else {
                    panic!("wrong message type")
                }

                pb.lock().await.inc(1);
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(
            format!("Corrected count rates for {pattern}").as_str(),
        ))
        .y_axis(Axis::new().title(Title::new("Count rate, Hz")))
        .height(1000);

    let (x, y, z, text) = count_rates
        .try_lock()
        .unwrap()
        .iter()
        .map(|(time, (counts, normed, filepath))| (*time, *counts, *normed, filepath.to_owned()))
        .unzip_n_vec();

    plot.add_trace(
        Scatter::new(x.clone(), y)
            .mode(Mode::Markers)
            .name("original count")
            .text_array(text),
    );

    plot.add_trace(Scatter::new(x, z).mode(Mode::Markers).name("corrected"));

    plot.set_layout(layout);

    plot.show();
}
