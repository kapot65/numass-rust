//! Построение спектра базовой линии для одной точки
//! Алгоритм делается для большой вейвформы

use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::Arc};

use egui::mutex::Mutex;
use plotly::{Plot, Scatter};

use unzip_n::unzip_n;

unzip_n!(2);

use processing::{
    histogram::PointHistogram, preprocess::extract_waveforms, storage::load_point
};

#[tokio::main]
async fn main() {

    let points = [
        // set 1
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p0(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p18(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p45(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p72(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p99(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p124(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p151(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_1/p178(30s)(HV1=14000)",

        // set 3
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_3/p0(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_3/p18(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_3/p45(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_3/p72(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_3/p99(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_3/p124(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_3/p151(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_3/p178(30s)(HV1=14000)",

        // set 5
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_5/p0(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_5/p18(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_5/p45(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_5/p72(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_5/p99(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_5/p124(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_5/p151(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_5/p178(30s)(HV1=14000)",

        // set 7
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_7/p0(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_7/p18(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_7/p45(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_7/p72(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_7/p99(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_7/p124(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_7/p151(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_7/p178(30s)(HV1=14000)",

        // set 9
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_9/p0(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_9/p18(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_9/p45(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_9/p72(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_9/p99(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_9/p124(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_9/p151(30s)(HV1=14000)",
        "/data-fast/numass-server/2024_11/Tritium_2_1/set_9/p178(30s)(HV1=14000)"
    ];

    let channel_baseline = Arc::new(Mutex::new(BTreeMap::new()));


    let futures = points.into_iter().map(|point_path| {

        let channel_baseline = Arc::clone(&channel_baseline);

        if !Path::new(point_path).exists() {
            println!("file {point_path} not found");
            return tokio::spawn(async move {});
        }

        tokio::spawn(async move {

            let point = load_point(&PathBuf::from(point_path)).await;

            let time = point.channels[0].blocks[0].time;

            let mut hist = PointHistogram::new_step(-5.0..120.0, 0.5);
            
            let waveforms = extract_waveforms(&point);

            let left = 6;
            let center = 15;
            let right = 6;

            for (_, frames) in waveforms {
                for (channel, waveform) in frames {

                    let filtered = waveform.windows(left + center + right).map(|window| {
                        (window[left+center..].iter().sum::<i16>() - window[..left].iter().sum::<i16>()) as f32 / (left + right) as f32
                    }).collect::<Vec<_>>();

                    hist.add_batch(channel, filtered);
                }
            }

            let x = hist.x.clone();


            let mut channel_baseline = channel_baseline.lock();

            for (ch, hist) in hist.channels {
                let mut max_idx = 0;
                for (idx, amp) in hist.iter().enumerate() {
                    if *amp > hist[max_idx] {
                        max_idx = idx;
                    }
                }

                let entry = channel_baseline.entry(ch).or_insert(vec![]);
                entry.push((time as u64, x[max_idx] as f64));
            }

        })
    }).collect::<Vec<_>>();


    for future in futures {
        future.await.unwrap();
    }

    let mut plot = Plot::new();

    for (ch, mut points) in channel_baseline.lock().clone().into_iter() {

        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        println!("ch# {}", ch + 1);
        println!("time\tbaseline");
        points.iter().for_each(|(time, baseline)| {
            println!("{}\t{}", time, baseline);
        });
        
        let (x, y) =  points.into_iter().unzip_n_vec();
        
        let line = Scatter::new(x, y)
        .mode(plotly::common::Mode::Markers).name(format!("ch# {}", ch + 1));

        plot.add_trace(line);
    }

    plot.show();
    
}
