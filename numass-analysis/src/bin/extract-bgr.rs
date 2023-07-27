use std::sync::Arc;

use plotly::{common::{Title, Line, LineShape}, layout::Axis, Layout, Plot, Scatter};
use protobuf::Message;

use dataforge::read_df_message;
use processing::{
    histogram::{PointHistogram, HistogramParams}, numass::{protos::rsb_event, NumassMeta}, extract_amplitudes, ProcessParams, amplitudes_to_histogram
};
use tokio::sync::Mutex;


#[tokio::main]
async fn main() {
    
    let db_root = "/data-nvme";
    let run = "2023_03";

    let range = 2.0..20.0;
    let bins = 180;

    // let pattern = format!("/{run}/Tritium_1/set_[4]/p*(HV1=16500)");
    let ethalon_pattern = format!("/{run}/Tritium_1/set_[1234]/p*(HV1=12000)");
    // let pattern = format!("/{run}/Tritium_1/set_[1234]/p*(HV1=16000)");
    // let pattern = format!("/{run}/Tritium_1/set_[123][1234567890]/p19(30s)(HV1=14000)");

    let samples = [
        ("trit-2-set-1", "/data-nvme/2023_03/Tritium_2/set_1/p118(30s)(HV1=12000)"),
        ("trit-2-set-15", "/data-nvme/2023_03/Tritium_2/set_15/p118(30s)(HV1=12000)"),
        ("trit-2-set-29", "/data-nvme/2023_03/Tritium_2/set_29/p118(30s)(HV1=12000)"),
        ("trit-3-set-1", "/data-nvme/2023_03/Tritium_3/set_1/p118(30s)(HV1=12000)"),
        ("trit-3-set-15", "/data-nvme/2023_03/Tritium_3/set_15/p118(30s)(HV1=12000)"),
        ("trit-3-set-30", "/data-nvme/2023_03/Tritium_3/set_30/p118(30s)(HV1=12000)"),
        ("trit-4-set-1", "/data-nvme/2023_03/Tritium_4/set_1/p118(30s)(HV1=12000)"),
        ("trit-4-set-15", "/data-nvme/2023_03/Tritium_4/set_15/p118(30s)(HV1=12000)"),
        ("trit-4-set-30", "/data-nvme/2023_03/Tritium_4/set_30/p118(30s)(HV1=12000)"),
        ("trit-5-set-1", "/data-nvme/2023_03/Tritium_4/set_1/p118(30s)(HV1=12000)"),
        ("trit-5-set-12", "/data-nvme/2023_03/Tritium_4/set_12/p118(30s)(HV1=12000)"),
    ];

    let range_l = 4.0..10.0;
    let range_r = 13.0..20.0;

    let eth_hist = {
        let eth_points = {
            
            let exclude: Vec<String> = vec![];
            analysis::get_points_by_pattern(db_root, &ethalon_pattern, &exclude)
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

    for (outfile, sample) in samples {
        let sample_hist = {
            let mut point_file = tokio::fs::File::open(sample).await.unwrap();
            let message = read_df_message::<NumassMeta>(&mut point_file)
                .await
                .unwrap();
            let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();
            
            amplitudes_to_histogram(extract_amplitudes(
                &point, 
                &ProcessParams::default()
            ), HistogramParams {
                range: range.clone(),
                bins
            })
        };
    
        let eth_counts = eth_hist.events_all(Some(range_l.clone())) 
        + eth_hist.events_all(Some(range_r.clone()))
        ;
        let sample_counts = sample_hist.events_all(Some(range_l.clone()))
         + sample_hist.events_all(Some(range_r.clone()))
        ;
        let ratio = sample_counts as f64 / eth_counts as f64;
    
        let mut plot = Plot::new();
    
        let layout = Layout::new()
        .title(Title::new(sample))
            .x_axis(Axis::new().title(Title::new("U_sp, kV")))
            .height(1000);
    
        plot.set_layout(layout);
        
        let y1 = eth_hist.merge_channels().into_iter().map(|val| val * ratio as f32).collect::<Vec<_>>();
    
        let difference = sample_hist.merge_channels().iter().zip(y1.iter()).map(|(sample, eth)| *sample - eth).collect::<Vec<_>>();
        
    
        let eth_shape = Scatter::new(
            eth_hist.x.clone(), y1
        ).line(Line::new().shape(LineShape::Hvh)).name("ethalon");
        plot.add_trace(eth_shape);
    
        let diff_shape = Scatter::new(
            eth_hist.x.clone(), difference
        ).line(Line::new().shape(LineShape::Hvh)).name("difference");
        plot.add_trace(diff_shape);
    
        let sample_shape = Scatter::new(
            sample_hist.x.clone(), sample_hist.merge_channels()
        ).line(Line::new().shape(LineShape::Hvh)).name("sample");
        plot.add_trace(sample_shape);
    
        plot.write_html(format!("{}.html", outfile));
        plot.write_image(format!("{}.png", outfile), plotly::ImageFormat::PNG, 1024, 768, 1.0)
    }
}