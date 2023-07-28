use std::path::PathBuf;

use analysis::{get_points_by_pattern, ethalon::get_ethalon};
use plotly::{common::{Title, Line, LineShape}, layout::Axis, Layout, Plot, Scatter};
use protobuf::Message;

use dataforge::read_df_message;
use processing::{
    histogram::HistogramParams, numass::{protos::rsb_event, NumassMeta}, extract_amplitudes, ProcessParams, amplitudes_to_histogram
};

#[tokio::main]
async fn main() {
    
    // ===== configuration =====
    let db_root = "/data-nvme";
    let run = "2023_03";

    let range = 2.0..20.0;
    let bins = 180;

    let voltage = 12000u16;
    let y_range = [-1000, 4000];

    // ===== processing code =====
    let out_folder = PathBuf::from(format!("workspace/extract-bgr/{voltage}"));
    std::fs::create_dir_all(&out_folder).unwrap();

    let mut paths = get_points_by_pattern(
        db_root, format!("/{run}/Tritium_[45]/set_*/p*(HV1={voltage})").as_str(), &[]).into_values().flatten().collect::<Vec<_>>();
    paths.sort_by(|a, b|
        natord::compare(a.to_str().unwrap(), b.to_str().unwrap())
    );

    let samples = paths.into_iter().enumerate().map(|(idx, point)| {
            (format!("trit-4-5-{}", idx), point)
        }
    ).collect::<Vec<_>>();

    let range_l = 4.0..(voltage as f32 / 1000.0 - 2.0);
    let range_r = (voltage as f32 / 1000.0 + 1.0)..20.0;
    
    let eth_hist = get_ethalon(voltage).await.unwrap();

    let handles = samples.iter().map(|(outfile, sample)| {

        let outfile = outfile.clone();
        let sample = sample.clone();
        let eth_hist = eth_hist.clone();
        let range = range.clone();
        let range_l = range_l.clone();
        let range_r = range_r.clone();
        let y_range = y_range;
        let out_folder = out_folder.clone();

        tokio::spawn(async move {
            let sample_hist = {
                let mut point_file = tokio::fs::File::open(&sample).await.unwrap();
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
            .title(Title::new(format!("{sample:?}").as_str()))
                .x_axis(
                    Axis::new().title(Title::new("U_sp, kV")
                ))
                .y_axis(Axis::new().range(y_range.to_vec()).title(Title::new("counts")))
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
        
            // plot.write_html(format!("{}.html", outfile));
            plot.write_image(out_folder.join(format!("{outfile}.png")) , plotly::ImageFormat::PNG, 1024, 768, 1.0)
        })
    }).collect::<Vec<_>>();

    for handle in handles {
        handle.await.unwrap();
    }
}