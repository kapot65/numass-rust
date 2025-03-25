//! Построение отношений счета в пикселях к счету в эталонном пикселе на протяжении времени.
//! Используется для оценки "уплывания" пучка.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use analysis::get_points_by_pattern;

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

unzip_n!(pub 2);

#[tokio::main]
async fn main() {
    let db_root = "/data-fast/numass-server";
    let pattern = "2024_11/Tritium_[45]/set_*[0-9]/p*(30s)(HV1=14000)";
    let exclude = [];

    let singles_range = 4.5..19.5;
    let process = ProcessParams {
        algorithm: TRAPEZOID_DEFAULT,
        convert_to_kev: true,
    };
    let postprocess = PostProcessParams::default();
    let main_channel = 6;

    let points = get_points_by_pattern(db_root, pattern, &exclude)
        .first_key_value()
        .unwrap()
        .1
        .clone();

    let ratios = Arc::new(Mutex::new(BTreeMap::new()));

    let pb: Arc<Mutex<indicatif::ProgressBar>> =
        Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));

    let handles = points
        .iter()
        .map(|filepath| {
            let filepath = filepath.clone();

            let ratios = Arc::clone(&ratios);

            let pb = Arc::clone(&pb);

            let main_channel = main_channel;

            let process = process.clone();
            let postprocess = postprocess.clone();

            let singles_range = singles_range.clone();

            tokio::spawn(async move {
                let (_, meta) = read_df_header_and_meta_sync::<NumassMeta>(
                    &mut std::fs::File::open(&filepath).unwrap(),
                )
                .unwrap();

                let point = load_point(&filepath).await;

                let (amps, _) = post_process(
                    extract_events(Some(meta.clone()), point, &process),
                    &postprocess,
                );

                let mut counts = BTreeMap::new();

                amps.values().for_each(|frames| {
                    frames.iter().for_each(|(_, event)| {
                        if let FrameEvent::Event {
                            amplitude, channel, ..
                        } = event
                        {
                            if singles_range.contains(amplitude) {
                                *counts.entry(*channel).or_insert(0) += 1;
                            }
                        }
                    });
                });

                let main_count = counts[&main_channel];

                let counts = counts
                    .iter()
                    .filter_map(|(key, value)| {
                        if *key != main_channel {
                            Some((*key, *value as f64 / main_count as f64))
                        } else {
                            None
                        }
                    })
                    .collect::<BTreeMap<_, _>>();

                if let NumassMeta::Reply(Reply::AcquirePoint { start_time, .. }) = meta {
                    ratios.lock().await.insert(start_time, counts);
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

    let ratios = ratios.lock().await.clone();

    let mut channels_presented = BTreeSet::new();
    ratios.iter().for_each(|(_, counts)| {
        counts.iter().for_each(|(ch, _)| {
            channels_presented.insert(ch);
        });
    });

    let graphs = channels_presented.into_iter().filter_map(|ch| {
        if ch == &main_channel {
            return None;
        }
        let (x, y) = ratios
            .iter()
            .filter_map(|(time, ratio)| {
                if !ratio.contains_key(ch) {
                    return None;
                }
                Some((time.to_owned(), ratio[ch]))
            })
            .unzip_n_vec();
        Some((ch, x, y))
    });

    let mut plot = Plot::new();
    let layout = Layout::new()
        .title(Title::new(
            format!("Pixel ratio for channels relative to channel {} for {pattern}", main_channel + 1)
                .as_str(),
        ))
        .y_axis(Axis::new().title(Title::new("Count rate, Hz")))
        .height(1000);
    graphs.for_each(|(ch, x, y)| {
        plot.add_trace(
            Scatter::new(x, y)
                .name(format!("{} / {}", ch + 1, main_channel + 1))
                .mode(Mode::Markers),
        );
    });
    plot.set_layout(layout);
    plot.show();
}
