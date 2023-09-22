use std::{collections::BTreeMap, sync::Arc};

use analysis::{get_points_by_pattern, CorrectionCoeffs};
use chrono::NaiveDateTime;
use dataforge::read_df_message;
use plotly::{Plot, Layout, common::{Title, Mode}, layout::Axis, Scatter};
use processing::{numass::{NumassMeta, protos::rsb_event}, post_process, extract_amplitudes, PostProcessParams, ProcessParams};
use protobuf::Message;
use tokio::sync::Mutex;

use unzip_n::unzip_n;

unzip_n!(pub 3);


#[tokio::main]
async fn main() {

    let db_root = "/data-ssd";
    // let db_root = "/data/numass-server";
    let run = "2023_03";
    // let pattern = format!("/{run}/Tritium_2/set_*/p*(30s)(HV1=14000)");
    let pattern = format!("/{run}/Tritium_5/set_*/p*(30s)(HV1=14000)");
    let exclude = [];

    let points = get_points_by_pattern(db_root, &pattern, &exclude).first_key_value().unwrap().1.clone();
    let coeffs = Arc::new(CorrectionCoeffs::load(&format!("/{db_root}/monitor.json")));
    let count_rates =  Arc::new(Mutex::new(BTreeMap::<NaiveDateTime, (usize, f32)>::new()));

    let pb: Arc<Mutex<indicatif::ProgressBar>> = Arc::new(Mutex::new(indicatif::ProgressBar::new(points.len() as u64)));
    let handles = points.iter().map(|filepath| {
        let filepath = filepath.clone();

        let count_rates = Arc::clone(&count_rates);
        let pb = Arc::clone(&pb);
        let coeffs = Arc::clone(&coeffs)    ;

        tokio::spawn(async move {
            let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
            let message = read_df_message::<NumassMeta>(&mut point_file)
                .await
                .unwrap();

            let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
            let k = coeffs.get_for_point(&filepath, &point);

            let amps = post_process(extract_amplitudes(
                &point, 
                &ProcessParams::default(),
            ), &PostProcessParams::default());

            let count_rate = amps.values().map(|frames| {
                frames.len()
            }).sum::<usize>();

            count_rates.lock().await.insert(
                chrono::NaiveDateTime::from_timestamp_opt((point.channels.first().unwrap().blocks.first().unwrap().time / 
                1_000_000_000) as i64, 0).unwrap(), 
                (count_rate, count_rate as f32 * k)
            );            
            
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

    let (x, y, z) =  count_rates.try_lock().unwrap().iter().map(|(time, (counts, normed))| {
        (*time, *counts, *normed)
    }).unzip_n_vec();

    plot.add_trace(
        Scatter::new(x.clone(), y)
        .mode(Mode::Markers).name("original count")
    );
    plot.add_trace(
        Scatter::new(x, z)
        .mode(Mode::Markers).name("corrected")
    );
    plot.set_layout(layout);

    plot.show();
}