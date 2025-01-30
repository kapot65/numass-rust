//! Построение спектра базовой линии для одной точки
//! Алгоритм делается для большой вейвформы

use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::Arc};

use egui::mutex::Mutex;
use plotly::{Plot, Scatter};

use unzip_n::unzip_n;

unzip_n!(2);

use processing::{
    histogram::PointHistogram, process::extract_waveforms, storage::load_point
};

#[tokio::main]
async fn main() {

    let points = [
        // "/data-nvme/2024_03/Electrode_2/set_1/p5(400s)(HV1=12000)",

        "/data-fast/numass-server/2024_11/Electrode_1/set_2/p0(400s)(HV1=4000)",

        "/data-fast/numass-server/2024_11/Electrode_1/set_5/p0(400s)(HV1=4000)",
        
        
        // "/data-nvme/2024_03/Tritium_2/set_1/p150(30s)(HV1=12050)",


        // "/data-nvme/2024_03/Tritium_2/set_3/p152(30s)(HV1=12000)",


        // "/data-nvme/2024_03/Tritium_2/set_5/p152(30s)(HV1=12000)",


        // "/data-2/numass-server/2024_03/Tritium_3/set_3/p152(30s)(HV1=12000)",
        // // "/data-2/numass-server/2024_03/Tritium_3/set_8/p152(30s)(HV1=12000)",

        // "/data-2/numass-server/2024_03/Tritium_4/set_1/p152(30s)(HV1=12000)",
        // // "/data-2/numass-server/2024_03/Tritium_4/set_6/p152(30s)(HV1=12000)",

        // "/data-2/numass-server/2024_03/Tritium_5/set_1/p152(30s)(HV1=12000)",
        // // "/data-2/numass-server/2024_03/Tritium_5/set_6/p152(30s)(HV1=12000)",

        // "/data-2/numass-server/2024_03/Tritium_5/set_1/p152(30s)(HV1=12000)",
        // // "/data-2/numass-server/2024_03/Tritium_5/set_6/p152(30s)(HV1=12000)",

        // "/data-2/numass-server/2024_03/Tritium_6/set_1/p152(30s)(HV1=12000)",
        // // "/data-2/numass-server/2024_03/Tritium_6/set_6/p152(30s)(HV1=12000)",
        // "/data-2/numass-server/2024_03/Tritium_6/set_12/p152(30s)(HV1=12000)",

        // // "/data-2/numass-server/2024_03/Tritium_7/set_1/p152(30s)(HV1=12000)",
        // "/data-2/numass-server/2024_03/Tritium_7/set_6/p152(30s)(HV1=12000)",
        // // "/data-2/numass-server/2024_03/Tritium_7/set_12/p152(30s)(HV1=12000)",
        // "/data-2/numass-server/2024_03/Tritium_7/set_18/p152(30s)(HV1=12000)",


        // // "/data-2/numass-server/2024_03/Tritium_8/set_1/p152(30s)(HV1=12000)",
        // "/data-3/numass-server/2024_03/Tritium_8/set_6/p152(30s)(HV1=12000)",

        // // "/data-3/numass-server/2024_03/Tritium_9/set_1/p152(30s)(HV1=12000)",
        // "/data-3/numass-server/2024_03/Tritium_9/set_6/p152(30s)(HV1=12000)",


        // // "/data-3/numass-server/2024_03/Tritium_10/set_1/p152(30s)(HV1=12000)",
        // "/data-3/numass-server/2024_03/Tritium_10/set_6/p152(30s)(HV1=12000)",

        // // "/data-3/numass-server/2024_03/Tritium_11/set_1/p152(30s)(HV1=12000)",
        // "/data-3/numass-server/2024_03/Tritium_11/set_6/p152(30s)(HV1=12000)",

        // // "/data-3/numass-server/2024_03/Tritium_12/set_1/p152(30s)(HV1=12000)",
        // "/data-3/numass-server/2024_03/Tritium_12/set_6/p152(30s)(HV1=12000)",
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
