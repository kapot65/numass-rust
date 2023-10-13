use std::sync::Arc;

use analysis::{get_points_by_pattern, ethalon::get_ethalon, CorrectionCoeffs, workspace::{get_workspace, get_db_fast_root, get_hist_range, get_hist_bins}};
use plotly::{common::{Title, Line, LineShape}, layout::Axis, Layout, Plot, Scatter};
use protobuf::Message;

use dataforge::read_df_message;
use processing::{
    histogram::HistogramParams, numass::{protos::rsb_event, NumassMeta, Reply, ExternalMeta}, extract_events, ProcessParams, events_to_histogram, PostProcessParams, post_process
};

#[tokio::main]
async fn main() {
    
    // ===== configuration =====
    let db_root = get_db_fast_root().to_str().unwrap().to_owned();
    let run = "2023_03";

    let y_range = [-1000, 4000];

    let fill = 1;
    let set = 4;

    // ===== processing code =====
    let out_folder = get_workspace().join(format!("bgr-ratio/trit-{fill}-set-{set}-abs"));
    std::fs::create_dir_all(&out_folder).unwrap();

    let points = get_points_by_pattern(
        &db_root, format!("/{run}/Tritium_{fill}/set_{set}/p*").as_str(), &[]).into_values().flatten().collect::<Vec<_>>();

    let coeffs = Arc::new(CorrectionCoeffs::load(&format!("/{db_root}/monitor.json")));

    let handles = points.into_iter().map(|filepath| {

        let y_range = y_range;
        let out_folder = out_folder.clone();
        let coeffs = Arc::clone(&coeffs);

        tokio::spawn(async move {
            let mut point_file = tokio::fs::File::open(&filepath).await.unwrap();
            let message = read_df_message::<NumassMeta>(&mut point_file)
                .await
                .unwrap();

            let voltage = if let NumassMeta::Reply(Reply::AcquirePoint { external_meta: Some(ExternalMeta {
                hv1_value: Some(voltage), ..
            }), .. }) = message.meta {
                voltage
            } else {
                panic!("wrong message type")
            };

            let voltage_kev = voltage as f32 / 1000.0;

            let range_l = 4.0..(voltage_kev - 4.0);
            let range_r = (voltage_kev + 1.0)..20.0;

            let point = rsb_event::Point::parse_from_bytes(&message.data.unwrap()[..]).unwrap();

            let monitor_coeff = coeffs.get_for_point(&filepath, &point);
            // println!("{filepath:?} -> {monitor_coeff}");

            let sample_hist = events_to_histogram(post_process(
                extract_events(
                    &point, 
                    &ProcessParams::default()), &PostProcessParams::default()
            ), HistogramParams {
                range: get_hist_range(),
                bins: get_hist_bins()
            });

            let eth_hist = get_ethalon(
                format!("/{run}/Tritium_1/set_[1234]/p*(HV1={voltage})"),
                ProcessParams::default(),
                PostProcessParams::default()
            ).await.unwrap();

            let ratio = {
                let eth_counts = 
                    eth_hist.events_all(Some(range_l.clone())) +
                    eth_hist.events_all(Some(range_r.clone()))
                ;
                let sample_counts = 
                    sample_hist.events_all(Some(range_l.clone())) +
                    sample_hist.events_all(Some(range_r.clone()))
                ;
                sample_counts as f64 / eth_counts as f64
            };

            // let main_range = Some((voltage_kev - 2.0)..(voltage_kev + 1.0));
            let main_range = Some(4.0..40.0);
            // let main_range = None;

            let eth_total = eth_hist.events_all(main_range.clone()) as f32 * ratio as f32;
            let sample_total = sample_hist.events_all(main_range.clone()) as f32;
        
            let mut plot = Plot::new();
        
            let layout = Layout::new()
            .title(Title::new(format!("{filepath:?}").as_str()))
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
            ).line(Line::new().shape(LineShape::Hvh)).name(format!("ethalon ({ratio})"));
            plot.add_trace(eth_shape);
        
            let diff_shape = Scatter::new(
                eth_hist.x, difference
            ).line(Line::new().shape(LineShape::Hvh)).name("difference");
            plot.add_trace(diff_shape);
        
            let sample_shape = Scatter::new(
                sample_hist.x.clone(), sample_hist.merge_channels()
            ).line(Line::new().shape(LineShape::Hvh)).name("sample");
            plot.add_trace(sample_shape);
        
            // plot.write_html(format!("{}.html", outfile));
            plot.write_image(out_folder.join(format!("{voltage}.png")) , plotly::ImageFormat::PNG, 1024, 768, 1.0);

            (voltage, (
                ((sample_total / eth_total) - 1.0) 
                * sample_hist.events_all(main_range) as f32 * monitor_coeff
            ))   
        })
    }).collect::<Vec<_>>();

    let mut out = "voltage\tratio\n".to_string();
    for handle in handles {
        let (voltage, ratio) = handle.await.unwrap();
        out.push_str(format!("{}\t{}\n", voltage, ratio).replace('.', ".").as_str());
    }
    tokio::fs::write(out_folder.join("ratios.tsv"), out).await.unwrap();
}