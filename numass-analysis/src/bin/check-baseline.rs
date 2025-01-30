use std::{path::PathBuf, sync::{Arc, Mutex}, collections::BTreeMap};

use dataforge::DFMessage;
use plotly::{Scatter, common::{ErrorData, ErrorType}, Plot};
use processing::{numass::{self, protos::rsb_event::Point}, preprocess::{emulate_fir, frame_to_waveform, Preprocess}, process::{Algorithm, TRAPEZOID_DEFAULT}, storage::load_meta, types::RawWaveform};
use protobuf::Message;

use statrs::statistics::Statistics;
use unzip3::Unzip3;

#[tokio::main]
async fn main() {

    let data_root = PathBuf::from("/data-fast/numass-server/2024_11/Tritium_2_1/set_2");
    let baseline_plot = Arc::new(Mutex::new(BTreeMap::new()));

    let pb = Arc::new(Mutex::new(indicatif::ProgressBar::new(std::fs::read_dir(&data_root).unwrap().count() as u64)));
    let handles = std::fs::read_dir(&data_root).unwrap().flatten().map(|file| {
        let baseline_plot = Arc::clone(&baseline_plot);
        let pb = Arc::clone(&pb);
        tokio::spawn(async move {
            if let Ok(DFMessage { meta: numass::Reply::AcquirePoint { 
                start_time, ..}, 
                data: Some(data) 
            }) = dataforge::read_df_message::<numass::Reply>(&mut tokio::fs::File::open(file.path()).await.unwrap()).await {
                // println!("{file:?}")


                let point = Point::parse_from_bytes(&data).unwrap();

                let meta = load_meta(&file.path()).await;
                let preprocess = Preprocess::from_point(meta, &point, &TRAPEZOID_DEFAULT);
                let baseline = preprocess.baseline.unwrap();
                println!("{baseline:?}");

                point.channels.iter().for_each(|ch| {
                    let baseline = ch.blocks.iter().flat_map(|block| {
                        block.frames.iter().flat_map(|frame| {
                            if let Algorithm::Trapezoid {
                                left, center, right, ..
                            } = TRAPEZOID_DEFAULT {
                                emulate_fir(frame_to_waveform(frame), right, center, left)
                            } else {
                                panic!("Unsupported algorithm")
                            }
                        })
                    }).map(|val| (val - baseline[ch.id as usize]) as f64).collect::<Vec<_>>();
                    if !baseline.is_empty() {
                        let dev = baseline.as_slice().std_dev();
                        let mean = baseline.mean();

                        let mut baseline_plot =  baseline_plot.lock().unwrap();
                        baseline_plot.entry(ch.id as u8).or_insert(vec![]).push((start_time, mean, dev))
                    }
                });
                pb.lock().unwrap().inc(1)
            }
        })
    }).collect::<Vec<_>>();
    
    for handle in handles {
        handle.await.unwrap();
    }
    pb.lock().unwrap().finish();

    let mut plot = Plot::new();

    for (ch_name, data) in baseline_plot.lock().unwrap().clone() {

        let mut data = data.clone();
        data.sort_by_key(|(time, _, _)| *time);

        let (x, y, err) = data.into_iter().unzip3::<Vec<_>, Vec<f64>, Vec<f64>>();
        
        let trace = Scatter::new(x, y)
        .name(format!("ch# {ch_name}"))
        .error_y(ErrorData::new(ErrorType::Data).array(err));

        plot.add_trace(trace);
    }

    let layout = plotly::Layout::new()
        .title(plotly::common::Title::new(data_root.to_str().unwrap()))
        .height(1000);
    plot.set_layout(layout);
    plot.show();
}