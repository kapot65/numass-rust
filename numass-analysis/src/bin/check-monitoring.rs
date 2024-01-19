use std::{collections::BTreeMap, sync::Arc};

use analysis::{get_points_by_pattern, CorrectionCoeffs, amps::get_amps};
use chrono::NaiveDateTime;
use dataforge::read_df_header_and_meta_sync;
use plotly::{Plot, Layout, common::{Title, Mode}, layout::Axis, Scatter};
use processing::{numass::{NumassMeta,  Reply}, postprocess::{post_process, PostProcessParams}, process::ProcessParams};
use tokio::sync::Mutex;

use unzip_n::unzip_n;

unzip_n!(pub 4);


#[tokio::main]
async fn main() {

    let db_root = "/data-nvme";
    // let db_root = "/data/numass-server";
    let run = "2023_11";
    // let pattern = format!("/{run}/Tritium_2/set_*/p*(30s)(HV1=14000)");
    let pattern = format!("/{run}/Tritium_14-18.6_[345]/set_*/p*(30s)(HV1=16000)");
    let exclude = [];

    let points = get_points_by_pattern(db_root, &pattern, &exclude).first_key_value().unwrap().1.clone();
    let coeffs = Arc::new(CorrectionCoeffs::load(&format!("/{db_root}/monitor-2023-11.json")));
    let count_rates =  Arc::new(Mutex::new(BTreeMap::<NaiveDateTime, (usize, f32, String)>::new()));

    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));

    let handles = points.iter().map(|filepath| {
        let filepath = filepath.clone();

        let count_rates = Arc::clone(&count_rates);
        let pb = Arc::clone(&pb);
        let coeffs = Arc::clone(&coeffs)    ;

        tokio::spawn(async move {

            let (_, meta) = read_df_header_and_meta_sync::<NumassMeta>(
                &mut std::fs::File::open(&filepath).unwrap()).unwrap();

            let k = coeffs.get_from_meta(&filepath, &meta);

            let amps = post_process(get_amps(
                &filepath,
                &ProcessParams::default(),
            ).await.unwrap(), &PostProcessParams::default());

            let count_rate = amps.values().map(|frames| {
                frames.iter().filter(|(_, (_, amp))| {
                    (6.0..40.0).contains(amp)
                }).collect::<Vec<_>>().len()
            }).sum::<usize>();


            if let NumassMeta::Reply(Reply::AcquirePoint { start_time, .. }) = meta {
                count_rates.lock().await.insert(
                    start_time, 
                    (count_rate, count_rate as f32 * k, filepath.to_str().unwrap().to_owned())
                );
            } else {
                panic!("wrong message type")
            }

            pb.lock().await.inc(1);
        })
    }).collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }

    let mut plot = Plot::new();

    let layout = Layout::new()
        .title(Title::new(
            format!("Corrected count rates for {pattern}")
                .as_str(),
        ))
        .x_axis(Axis::new().title(Title::new("Counts")))
        .height(1000);

    let (x, y, z, text) =  count_rates.try_lock().unwrap().iter().map(|(time, (counts, normed, filepath))| {
        (*time, *counts, *normed, filepath.to_owned())
    }).unzip_n_vec();

    plot.add_trace(
        Scatter::new(x.clone(), y)
        .mode(Mode::Markers).name("original count")
        .text_array(text)
    );
    plot.add_trace(
        Scatter::new(x, z)
        .mode(Mode::Markers).name("corrected")
    );
    
    plot.set_layout(layout);

    plot.show();
}